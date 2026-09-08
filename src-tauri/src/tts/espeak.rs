// SPEC: read-aloud (TTS-38)

//! Turning text into phonemes with espeak-ng, called directly.
//!
//! **Why this file exists instead of a crate.** The Rust bindings for espeak-ng
//! (`espeak-rs-sys`) generate their headers with bindgen, which needs LLVM on
//! every machine that compiles the app. Measured on 2026-09-07: `cargo install
//! kokoro-cli` failed here with *"Unable to find libclang"*, and the fix would
//! have been a second build prerequisite next to the `protoc` that `lancedb`
//! already imposes - for the user, for any contributor, and for CI.
//!
//! The whole surface needed is **three C functions**. Declaring them by hand
//! costs the twenty lines below and no prerequisite at all.
//!
//! **The library is already in the bundle.** `espeak-ng.dll` and its
//! `espeak-ng-data` ship inside the piper component (380.928 bytes), because
//! piper phonemizes with the same engine. Nothing new is downloaded.
//!
//! The call sequence is not invented: it was read out of `phonemizer`, the
//! Python package the reference implementation of Kokoro uses -
//! `espeak_Initialize(0x02, 0, data_path, 0)`, then `espeak_SetVoiceByName`,
//! then `espeak_TextToPhonemes` with `text_mode = 1` (UTF-8) and
//! `phoneme_mode = ('_' << 8) | 0x02` (IPA, separated by `_`).

use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;
use std::sync::Mutex;

/// `AUDIO_OUTPUT_SYNCHRONOUS`. Nothing is played: this app only ever asks
/// espeak for phonemes, and the audio comes from Kokoro.
const AUDIO_OUTPUT_SYNCHRONOUS: c_int = 0x02;

/// The input is UTF-8.
const CHARS_UTF8: c_int = 1;

/// IPA output (`0x02`), with `_` between phonemes (`'_' << 8`). The separator
/// is dropped afterwards - it is not in Kokoro's vocabulary - but asking for it
/// is what keeps two adjacent phonemes from being read as one symbol.
const PHONEMES_IPA_UNDERSCORE: c_int = (b'_' as c_int) << 8 | 0x02;

/// espeak-ng keeps **process-global state**: the current voice is a global, and
/// `TextToPhonemes` walks a cursor through the text it was given. Two threads
/// phonemizing at once corrupt each other's output.
///
/// The reference implementation guards it with a lock for exactly this reason,
/// and the comment there is blunt about it ("concurrent phonemization returns
/// corrupted phonemes"). This is that lock.
static ESPEAK: Mutex<Option<Engine>> = Mutex::new(None);

struct Engine {
    /// Kept alive for as long as the symbols below are used: dropping the
    /// `Library` unloads the DLL under them.
    _library: Library,
    set_voice: unsafe extern "C" fn(*const c_char) -> c_int,
    to_phonemes: unsafe extern "C" fn(*mut *const c_void, c_int, c_int) -> *const c_char,
    /// The voice currently set, so a second sentence in the same language does
    /// not pay for the switch.
    voice: String,
}

/// Punctuation that survives into the phonemes.
///
/// **Not decoration.** These characters are in Kokoro's vocabulary, and they
/// are what give the voice its pause and its falling intonation at the end of a
/// sentence. `espeak_TextToPhonemes` drops them, so a sentence phonemized raw
/// comes back as an unbroken run of sounds and is read flat.
///
/// The reference implementation gets this from `phonemizer`'s
/// `preserve_punctuation=True`, which is Python-side logic and not something
/// espeak does. This is that logic, and the parity test is what proves the two
/// agree.
const KEPT_PUNCTUATION: [char; 10] = ['.', ',', ';', ':', '!', '?', '"', '(', ')', '—'];

/// Phonemizes `text` in `language`, as IPA, keeping the punctuation in place.
///
/// `language` is an espeak voice name: `pt-br`, `en-us`, `es`. The mapping from
/// the app's language codes lives in the caller, not here.
pub fn phonemize(library: &Path, data_dir: &Path, language: &str, text: &str) -> Result<String, String> {
    // The text is cut on punctuation, each run is phonemized on its own, and
    // the marks go back exactly where they were. Sending the whole sentence and
    // trying to put the marks back afterwards would need to know where a
    // phoneme run started in the original - which nothing here can know.
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for ch in text.chars() {
        if KEPT_PUNCTUATION.contains(&ch) {
            if !run.trim().is_empty() {
                out.push_str(&phonemize_run(library, data_dir, language, &run)?);
            }
            run.clear();
            out.push(ch);
        } else {
            run.push(ch);
        }
    }
    if !run.trim().is_empty() {
        out.push_str(&phonemize_run(library, data_dir, language, &run)?);
    }
    Ok(out)
}

fn phonemize_run(library: &Path, data_dir: &Path, language: &str, text: &str) -> Result<String, String> {
    // The space around a run is part of the sentence's rhythm: "claro, sim"
    // keeps the space after the comma, and losing it glues two words into one.
    let leading = if text.starts_with(char::is_whitespace) { " " } else { "" };
    let trailing = if text.ends_with(char::is_whitespace) { " " } else { "" };
    let spoken = phonemize_raw(library, data_dir, language, text.trim())?;
    Ok(format!("{leading}{spoken}{trailing}"))
}

fn phonemize_raw(library: &Path, data_dir: &Path, language: &str, text: &str) -> Result<String, String> {
    let mut guard = ESPEAK.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(load(library, data_dir)?);
    }
    let engine = guard.as_mut().expect("just loaded");

    if engine.voice != language {
        let name = CString::new(language).map_err(|e| e.to_string())?;
        // SAFETY: `name` outlives the call, and espeak copies what it needs.
        let code = unsafe { (engine.set_voice)(name.as_ptr()) };
        if code != 0 {
            return Err(format!("espeak não conhece a voz '{language}' (código {code})"));
        }
        engine.voice = language.to_string();
    }

    let source = CString::new(text.replace('\0', " ")).map_err(|e| e.to_string())?;
    let mut cursor = source.as_ptr() as *const c_void;
    let mut out = String::with_capacity(text.len());

    // `TextToPhonemes` advances the pointer and returns one chunk per call,
    // ending when the pointer is null. A loop, not a single call - a sentence
    // longer than espeak's internal buffer comes back in pieces, and taking
    // only the first would silently truncate the reading.
    while !cursor.is_null() {
        // SAFETY: `cursor` points into `source`, which outlives the loop, and
        // espeak owns the returned buffer until the next call - it is copied
        // before anything else touches the library.
        let chunk = unsafe { (engine.to_phonemes)(&mut cursor, CHARS_UTF8, PHONEMES_IPA_UNDERSCORE) };
        if chunk.is_null() {
            break;
        }
        let piece = unsafe { CStr::from_ptr(chunk) }
            .to_str()
            .map_err(|e| format!("espeak devolveu fonemas inválidos: {e}"))?;
        if !out.is_empty() && !piece.is_empty() {
            out.push(' ');
        }
        out.push_str(piece);
    }
    Ok(out)
}

/// Drops the library, so one test case cannot leave a voice set for the next.
///
/// `cfg(test)` because that is the only reason it exists: in production the
/// library is loaded once and stays, which is the point of caching it.
#[cfg(test)]
pub fn reset() {
    let mut guard = ESPEAK.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;
}

fn load(library: &Path, data_dir: &Path) -> Result<Engine, String> {
    if !library.exists() {
        return Err(format!(
            "componente 'espeak-ng' não encontrado em {} — reinstale o ReadMe",
            library.display()
        ));
    }
    // SAFETY: loading a shared library runs its initialisers. This one ships
    // inside our own bundle, resolved by `bundled::find_file`, never a path
    // from outside.
    let lib = unsafe { Library::new(library) }
        .map_err(|e| format!("não foi possível carregar o espeak-ng: {e}"))?;

    let data = CString::new(data_dir.to_string_lossy().as_ref()).map_err(|e| e.to_string())?;
    unsafe {
        let initialize: Symbol<unsafe extern "C" fn(c_int, c_int, *const c_char, c_int) -> c_int> =
            lib.get(b"espeak_Initialize\0")
                .map_err(|e| format!("espeak_Initialize ausente: {e}"))?;
        // Returns the sample rate on success, <= 0 on failure. The data path is
        // where the language dictionaries live; without it every voice fails.
        if initialize(AUDIO_OUTPUT_SYNCHRONOUS, 0, data.as_ptr(), 0) <= 0 {
            return Err(format!(
                "espeak-ng não inicializou com os dados em {}",
                data_dir.display()
            ));
        }
        let set_voice: Symbol<unsafe extern "C" fn(*const c_char) -> c_int> = lib
            .get(b"espeak_SetVoiceByName\0")
            .map_err(|e| format!("espeak_SetVoiceByName ausente: {e}"))?;
        let to_phonemes: Symbol<
            unsafe extern "C" fn(*mut *const c_void, c_int, c_int) -> *const c_char,
        > = lib
            .get(b"espeak_TextToPhonemes\0")
            .map_err(|e| format!("espeak_TextToPhonemes ausente: {e}"))?;

        let set_voice = *set_voice;
        let to_phonemes = *to_phonemes;
        Ok(Engine {
            _library: lib,
            set_voice,
            to_phonemes,
            voice: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paridade com a implementação de referência, contra a biblioteca real.
    ///
    /// A string esperada **não foi escrita à mão**: saiu do `kokoro-onnx` em
    /// Python, o pacote que o próprio projeto publica, rodado nesta máquina em
    /// 2026-09-07 sobre a mesma frase. Se a FFI aqui divergir dele, a leitura
    /// sai com sotaque errado — e é exatamente o defeito que ninguém pega
    /// lendo código, só ouvindo.
    ///
    /// Precisa da biblioteca de verdade, que só existe depois de `npm run
    /// vendor`, então vem por variável de ambiente, no formato que a AD-057
    /// estabeleceu:
    ///
    /// ```text
    /// READER_ESPEAK_LIBRARY=<repo>/src-tauri/resources/piper/piper/espeak-ng.dll
    /// READER_ESPEAK_DATA=<repo>/src-tauri/resources/piper/piper/espeak-ng-data
    /// cargo test --lib espeak -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn the_phonemes_match_the_reference_implementation() {
        let library = std::env::var("READER_ESPEAK_LIBRARY")
            .expect("set READER_ESPEAK_LIBRARY to espeak-ng.dll");
        let data = std::env::var("READER_ESPEAK_DATA")
            .expect("set READER_ESPEAK_DATA to the espeak-ng-data folder");
        reset();

        let out = phonemize(
            Path::new(&library),
            Path::new(&data),
            "pt-br",
            "Pelo menos é assim que o meu irmão disse que te chamam.",
        )
        .expect("a fonemização falhou");

        println!("fonemas: {out}");
        // O `_` separador sai; ele não está no vocabulário do Kokoro.
        let clean: String = out.chars().filter(|c| *c != '_').collect();
        assert_eq!(
            clean.split_whitespace().collect::<Vec<_>>().join(" "),
            "pˈelʊ mˈenʊz ɛ asˈiŋ ky ʊ meʊ iɾəmˈɐ̃ʊ̃ dʒˈisy ky tʃy ʃˈɐ̃mɐ̃ʊ̃.",
            "os fonemas divergiram da referência em Python"
        );
    }

    /// A pontuação é o que dá pausa e entonação, e o espeak cru a joga fora.
    /// Sem isto a frase sai como um bloco só de sons, lida sem respirar.
    #[test]
    #[ignore]
    fn punctuation_survives_because_it_is_the_prosody() {
        let library = std::env::var("READER_ESPEAK_LIBRARY").expect("READER_ESPEAK_LIBRARY");
        let data = std::env::var("READER_ESPEAK_DATA").expect("READER_ESPEAK_DATA");
        reset();

        let out = phonemize(
            Path::new(&library),
            Path::new(&data),
            "pt-br",
            "Sim, claro. E agora?",
        )
        .unwrap();

        println!("com pontuação: {out}");
        for mark in [',', '.', '?'] {
            assert!(out.contains(mark), "a pontuação {mark:?} sumiu: {out}");
        }
        // E a vírgula não pode ter colado as duas palavras.
        assert!(out.contains(", "), "o espaço depois da vírgula sumiu: {out}");
    }

    /// Trocar de idioma no meio tem de trocar de verdade: a voz do espeak é
    /// estado global, e um segundo idioma lido com a voz do primeiro sairia
    /// com a fonética errada sem erro nenhum.
    #[test]
    #[ignore]
    fn switching_language_changes_the_phonemes() {
        let library = std::env::var("READER_ESPEAK_LIBRARY").expect("READER_ESPEAK_LIBRARY");
        let data = std::env::var("READER_ESPEAK_DATA").expect("READER_ESPEAK_DATA");
        reset();
        let word = "the letter";

        let pt = phonemize(Path::new(&library), Path::new(&data), "pt-br", word).unwrap();
        let en = phonemize(Path::new(&library), Path::new(&data), "en-us", word).unwrap();

        println!("pt-br: {pt}\nen-us: {en}");
        assert_ne!(pt, en, "o idioma não trocou: o mesmo texto deu os mesmos fonemas");
    }

    #[test]
    fn a_missing_library_names_the_component_instead_of_panicking() {
        reset();
        let err = phonemize(
            Path::new("nao-existe-espeak.dll"),
            Path::new("."),
            "pt-br",
            "olá",
        )
        .unwrap_err();
        assert!(err.contains("espeak-ng"), "não nomeou o componente: {err}");
        assert!(err.contains("reinstale"), "não disse o que fazer: {err}");
    }
}
