// SPEC: read-aloud (TTS-39, TTS-40, TTS-41)

//! Kokoro: phonemes in, speech out.
//!
//! The model is one ONNX graph and one bag of voices, and it runs on the
//! **ONNX Runtime the app already loads** for embeddings - no second runtime,
//! no new binary in the installer. The phonemes come from `tts::espeak`, which
//! calls the espeak-ng that already ships inside the piper component.
//!
//! **Everything below was read out of the reference implementation, not
//! guessed.** `kokoro-onnx`, the Python package the project publishes, was
//! installed and read on 2026-09-07, and these are the four facts that matter:
//!
//! 1. the token input is `[0, ...ids, 0]` - a zero at each end, shape `[1, n]`;
//! 2. the style vector is row `len(ids) - 1` of the voice, so a voice is a
//!    table of 510 styles, one per possible phoneme count;
//! 3. `speed` is a single float, and the model was trained for 0,5 to 2,0;
//! 4. the output is f32 samples at 24.000 Hz - higher than piper's 22.050.
//!
//! The voices file is an **NPZ**: a zip of `.npy` arrays, one per voice, each
//! `(510, 1, 256)` f32. The app already depends on `zip`, so reading it needs
//! no dependency - just the little `.npy` header parser below.

use super::espeak;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Kokoro's output rate. Not a choice: it is what the model produces.
pub const SAMPLE_RATE: u32 = 24_000;

/// The model refuses more than this many phonemes in one pass, so a long
/// sentence is split before it gets here.
pub const MAX_PHONEMES: usize = 510;

/// Every voice is a table of this many style vectors, indexed by phoneme count.
const STYLES_PER_VOICE: usize = 510;
const STYLE_DIM: usize = 256;

/// Phoneme to token id, exactly as the model was trained.
///
/// 114 entries, generated from the reference implementation's `config.json`
/// rather than typed: a single wrong id here would not fail, it would just make
/// the voice mispronounce one sound, and nothing would ever report it.
const VOCAB: [(char, i64); 114] = [
    (';', 1),
    (':', 2),
    (',', 3),
    ('.', 4),
    ('!', 5),
    ('?', 6),
    ('—', 9),
    ('…', 10),
    ('"', 11),
    ('(', 12),
    (')', 13),
    ('“', 14),
    ('”', 15),
    (' ', 16),
    ('̃', 17),
    ('ʣ', 18),
    ('ʥ', 19),
    ('ʦ', 20),
    ('ʨ', 21),
    ('ᵝ', 22),
    ('ꭧ', 23),
    ('A', 24),
    ('I', 25),
    ('O', 31),
    ('Q', 33),
    ('S', 35),
    ('T', 36),
    ('W', 39),
    ('Y', 41),
    ('ᵊ', 42),
    ('a', 43),
    ('b', 44),
    ('c', 45),
    ('d', 46),
    ('e', 47),
    ('f', 48),
    ('h', 50),
    ('i', 51),
    ('j', 52),
    ('k', 53),
    ('l', 54),
    ('m', 55),
    ('n', 56),
    ('o', 57),
    ('p', 58),
    ('q', 59),
    ('r', 60),
    ('s', 61),
    ('t', 62),
    ('u', 63),
    ('v', 64),
    ('w', 65),
    ('x', 66),
    ('y', 67),
    ('z', 68),
    ('ɑ', 69),
    ('ɐ', 70),
    ('ɒ', 71),
    ('æ', 72),
    ('β', 75),
    ('ɔ', 76),
    ('ɕ', 77),
    ('ç', 78),
    ('ɖ', 80),
    ('ð', 81),
    ('ʤ', 82),
    ('ə', 83),
    ('ɚ', 85),
    ('ɛ', 86),
    ('ɜ', 87),
    ('ɟ', 90),
    ('ɡ', 92),
    ('ɥ', 99),
    ('ɨ', 101),
    ('ɪ', 102),
    ('ʝ', 103),
    ('ɯ', 110),
    ('ɰ', 111),
    ('ŋ', 112),
    ('ɳ', 113),
    ('ɲ', 114),
    ('ɴ', 115),
    ('ø', 116),
    ('ɸ', 118),
    ('θ', 119),
    ('œ', 120),
    ('ɹ', 123),
    ('ɾ', 125),
    ('ɻ', 126),
    ('ʁ', 128),
    ('ɽ', 129),
    ('ʂ', 130),
    ('ʃ', 131),
    ('ʈ', 132),
    ('ʧ', 133),
    ('ʊ', 135),
    ('ʋ', 136),
    ('ʌ', 138),
    ('ɣ', 139),
    ('ɤ', 140),
    ('χ', 142),
    ('ʎ', 143),
    ('ʒ', 147),
    ('ʔ', 148),
    ('ˈ', 156),
    ('ˌ', 157),
    ('ː', 158),
    ('ʰ', 162),
    ('ʲ', 164),
    ('↓', 169),
    ('→', 171),
    ('↗', 172),
    ('↘', 173),
    ('ᵻ', 177),
];

fn vocab() -> &'static HashMap<char, i64> {
    static MAP: OnceLock<HashMap<char, i64>> = OnceLock::new();
    MAP.get_or_init(|| VOCAB.iter().copied().collect())
}

/// The phonemes that survive, as token ids.
///
/// Anything outside the vocabulary is dropped - the separator espeak puts
/// between phonemes included. That is what the reference does, and it is what
/// keeps an unknown symbol from becoming a wrong sound.
pub fn tokenize(phonemes: &str) -> Vec<i64> {
    let map = vocab();
    phonemes.chars().filter_map(|c| map.get(&c).copied()).collect()
}

/// One voice: 510 style vectors of 256 floats.
pub struct Voice {
    styles: Vec<f32>,
}

impl Voice {
    /// The style for a given phoneme count, which is row `count - 1`.
    ///
    /// Clamped at both ends: zero phonemes never reaches here, and a count past
    /// the table takes the last row - the same `min(len, rows) - 1` the
    /// reference uses.
    fn style_for(&self, count: usize) -> &[f32] {
        let row = count.clamp(1, STYLES_PER_VOICE) - 1;
        &self.styles[row * STYLE_DIM..(row + 1) * STYLE_DIM]
    }
}

/// Reads one voice out of the NPZ bundle.
///
/// The file holds all 54 voices, so it is opened per request rather than kept
/// in memory: 28 MB resident for a voice nobody is using is worse than a zip
/// seek per page of audio.
pub fn load_voice(bundle: &Path, name: &str) -> Result<Voice, String> {
    let file = std::fs::File::open(bundle)
        .map_err(|e| format!("não foi possível abrir o pacote de vozes: {e}"))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| format!("o pacote de vozes não é um arquivo válido: {e}"))?;

    // NPZ names its members `<key>.npy`; some writers omit the extension. The
    // name is resolved before the entry is opened, because holding one borrow
    // of the archive while asking for another does not compile - and would be a
    // real aliasing bug if it did.
    let member = if zip.index_for_name(&format!("{name}.npy")).is_some() {
        format!("{name}.npy")
    } else {
        name.to_string()
    };
    let mut entry = zip
        .by_name(&member)
        .map_err(|_| format!("a voz '{name}' não está no pacote"))?;
    let mut raw = Vec::new();
    entry
        .read_to_end(&mut raw)
        .map_err(|e| format!("não foi possível ler a voz '{name}': {e}"))?;
    parse_npy(&raw).map(|styles| Voice { styles })
}

/// The float payload of a `.npy` array.
///
/// Only the shape this model uses is accepted - little-endian f32. Refusing
/// anything else is deliberate: silently reinterpreting f64 or big-endian bytes
/// as f32 would produce a voice made of noise, and no error to explain it.
fn parse_npy(raw: &[u8]) -> Result<Vec<f32>, String> {
    if raw.len() < 10 || &raw[..6] != b"\x93NUMPY" {
        return Err("a voz não está no formato .npy".to_string());
    }
    // v1 uses 2 header-length bytes, v2 and up use 4.
    let (len_bytes, len_at) = if raw[6] >= 2 { (4usize, 8usize) } else { (2usize, 8usize) };
    let header_len = match len_bytes {
        2 => u16::from_le_bytes([raw[len_at], raw[len_at + 1]]) as usize,
        _ => u32::from_le_bytes([raw[len_at], raw[len_at + 1], raw[len_at + 2], raw[len_at + 3]])
            as usize,
    };
    let start = len_at + len_bytes;
    let header = std::str::from_utf8(raw.get(start..start + header_len).ok_or("cabeçalho .npy truncado")?)
        .map_err(|e| format!("cabeçalho .npy ilegível: {e}"))?;

    if !header.contains("'<f4'") && !header.contains("\"<f4\"") {
        return Err(format!("a voz não é float32 little-endian: {header}"));
    }
    if header.contains("'fortran_order': True") {
        return Err("a voz está em ordem Fortran, que este leitor não suporta".to_string());
    }

    let data = &raw[start + header_len..];
    if data.len() % 4 != 0 {
        return Err("os bytes da voz não são múltiplos de 4".to_string());
    }
    let floats: Vec<f32> = data
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    let expected = STYLES_PER_VOICE * STYLE_DIM;
    if floats.len() != expected {
        return Err(format!(
            "a voz tem {} floats, esperados {expected}",
            floats.len()
        ));
    }
    Ok(floats)
}

/// The loaded graph, kept for the life of the process.
///
/// ponytail: one session, one lock, like `rag::pdfium`'s. Two sentences are
/// never synthesised at once in this app, and the model is 310 MB - a session
/// per call would reload it every sentence.
static SESSION: Mutex<Option<ort::session::Session>> = Mutex::new(None);

/// Synthesises one sentence, returning f32 samples at [`SAMPLE_RATE`].
pub fn speak(
    model: &Path,
    voices_bundle: &Path,
    espeak_library: &Path,
    espeak_data: &Path,
    language: &str,
    voice_name: &str,
    text: &str,
    speed: f32,
) -> Result<Vec<f32>, String> {
    let phonemes = espeak::phonemize(espeak_library, espeak_data, language, text)?;
    let mut ids = tokenize(&phonemes);
    if ids.is_empty() {
        return Err(format!("nenhum fonema de {text:?} está no vocabulário do modelo"));
    }
    ids.truncate(MAX_PHONEMES);
    let voice = load_voice(voices_bundle, voice_name)?;
    let style = voice.style_for(ids.len()).to_vec();

    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let session = ort::session::Session::builder()
            .map_err(|e| format!("não foi possível preparar o modelo de voz: {e}"))?
            .commit_from_file(model)
            .map_err(|e| format!("não foi possível carregar o modelo de voz: {e}"))?;
        *guard = Some(session);
    }
    let session = guard.as_mut().expect("just built");

    // A zero at each end, exactly as the reference does. Without them the model
    // clips the first and last sound of every sentence.
    let mut tokens = Vec::with_capacity(ids.len() + 2);
    tokens.push(0i64);
    tokens.extend_from_slice(&ids);
    tokens.push(0i64);

    let token_count = tokens.len();
    let inputs = ort::inputs![
        "input_ids" => ort::value::Value::from_array(([1usize, token_count], tokens))
            .map_err(|e| e.to_string())?,
        "style" => ort::value::Value::from_array(([1usize, STYLE_DIM], style))
            .map_err(|e| e.to_string())?,
        "speed" => ort::value::Value::from_array(([1usize], vec![speed.clamp(0.5, 2.0)]))
            .map_err(|e| e.to_string())?,
    ];
    let outputs = session
        .run(inputs)
        .map_err(|e| format!("a geração de voz falhou: {e}"))?;
    let (_, audio) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("o modelo devolveu um áudio ilegível: {e}"))?;
    Ok(audio.to_vec())
}

/// Wraps samples in a WAV, so the rest of the app handles Kokoro and piper
/// through the same bytes.
pub fn to_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for sample in samples {
        // Clamped before the cast: a sample past 1.0 would wrap around and
        // become a loud click instead of a loud sound.
        let clamped = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&clamped.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vocabulary_is_the_one_the_model_was_trained_with() {
        // Gerado do `config.json` da implementação de referência, não digitado.
        // Um id errado aqui não falha: faz a voz pronunciar um som trocado, e
        // nada reporta.
        assert_eq!(VOCAB.len(), 114);
        assert_eq!(vocab().len(), 114, "há caracteres repetidos na tabela");
        // Âncoras conferidas contra a referência em 2026-09-07.
        assert_eq!(vocab().get(&';'), Some(&1));
        assert_eq!(vocab().get(&'.'), Some(&4));
        assert_eq!(vocab().get(&' '), Some(&16));
        // Os ids são únicos: dois fonemas no mesmo id seriam o mesmo som.
        let mut ids: Vec<i64> = VOCAB.iter().map(|(_, i)| *i).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "há ids repetidos no vocabulário");
    }

    #[test]
    fn the_portuguese_sentence_tokenizes_exactly_like_the_reference() {
        // Os fonemas e os tokens saíram do `kokoro-onnx` em Python, rodado
        // nesta máquina. É o mesmo par que o teste de paridade do `espeak`
        // usa, agora atravessando a segunda metade do caminho.
        let phonemes = "pˈelʊ mˈenʊz ɛ asˈiŋ ky ʊ meʊ iɾəmˈɐ̃ʊ̃ dʒˈisy ky tʃy ʃˈɐ̃mɐ̃ʊ̃.";
        let expected: Vec<i64> = vec![
            58, 156, 47, 54, 135, 16, 55, 156, 47, 56, 135, 68, 16, 86, 16, 43, 61, 156, 51, 112,
            16, 53, 67, 16, 135, 16, 55, 47, 135, 16, 51, 125, 83, 55, 156, 70, 17, 135, 17, 16,
            46, 147, 156, 51, 61, 67, 16, 53, 67, 16, 62, 131, 67, 16, 131, 156, 70, 17, 55, 70,
            17, 135, 17, 4,
        ];

        assert_eq!(tokenize(phonemes), expected);
    }

    #[test]
    fn a_symbol_outside_the_vocabulary_is_dropped_and_never_guessed() {
        // O separador que o espeak devolve é o caso real: ele não está no
        // vocabulário e some, em vez de virar um som qualquer.
        assert_eq!(tokenize("p_ˈe"), tokenize("pˈe"));
        assert!(tokenize("你好").is_empty());
    }

    #[test]
    fn the_wav_header_says_what_the_model_actually_produced() {
        let wav = to_wav(&[0.0; SAMPLE_RATE as usize]);

        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), SAMPLE_RATE);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1, "não é mono");
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16, "não é 16 bits");
        // Um segundo de áudio: o cabeçalho e o corpo têm de concordar.
        assert_eq!(wav.len(), 44 + SAMPLE_RATE as usize * 2);
    }

    #[test]
    fn a_sample_past_full_scale_is_clamped_instead_of_wrapping() {
        // Sem o clamp, +1.5 daria a volta e viraria um estalo alto — o defeito
        // clássico de converter float para inteiro em áudio.
        let wav = to_wav(&[1.5, -1.5]);
        let first = i16::from_le_bytes(wav[44..46].try_into().unwrap());
        let second = i16::from_le_bytes(wav[46..48].try_into().unwrap());
        assert!(first > 32_000, "estourou para o outro lado: {first}");
        assert!(second < -32_000, "estourou para o outro lado: {second}");
    }

    /// A única prova de que o Kokoro fala nesta máquina — e o número que diz
    /// quanto custa.
    ///
    /// Exige o modelo real (325 MB) e a runtime ONNX, então vem por variável de
    /// ambiente, no formato da AD-057:
    ///
    /// ```text
    /// READER_KOKORO_MODEL=%APPDATA%/com.readme.app/voices/kokoro-v1.0.onnx
    /// READER_KOKORO_VOICES=%APPDATA%/com.readme.app/voices/kokoro-voices-v1.0.bin
    /// READER_ESPEAK_LIBRARY=<repo>/src-tauri/resources/piper/piper/espeak-ng.dll
    /// READER_ESPEAK_DATA=<repo>/src-tauri/resources/piper/piper/espeak-ng-data
    /// README_ORT_DYLIB=<repo>/src-tauri/resources/onnxruntime/.../onnxruntime.dll
    /// cargo test --lib kokoro -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn one_sentence_is_synthesised_and_the_cost_is_measured() {
        let model = std::env::var("READER_KOKORO_MODEL").expect("READER_KOKORO_MODEL");
        let bundle = std::env::var("READER_KOKORO_VOICES").expect("READER_KOKORO_VOICES");
        let library = std::env::var("READER_ESPEAK_LIBRARY").expect("READER_ESPEAK_LIBRARY");
        let data = std::env::var("READER_ESPEAK_DATA").expect("READER_ESPEAK_DATA");
        let dylib = std::env::var("README_ORT_DYLIB").expect("README_ORT_DYLIB");
        std::env::set_var("ORT_DYLIB_PATH", &dylib);

        let sentence = "Vasculhei minha agenda mental, e ela estava vazia.";
        let started = std::time::Instant::now();
        let samples = speak(
            Path::new(&model),
            Path::new(&bundle),
            Path::new(&library),
            Path::new(&data),
            "pt-br",
            "pf_dora",
            sentence,
            1.0,
        )
        .expect("a síntese falhou");
        let cold = started.elapsed();

        let again = std::time::Instant::now();
        let _ = speak(
            Path::new(&model),
            Path::new(&bundle),
            Path::new(&library),
            Path::new(&data),
            "pt-br",
            "pf_dora",
            sentence,
            1.0,
        )
        .expect("a segunda síntese falhou");
        let warm = again.elapsed();

        let seconds = samples.len() as f32 / SAMPLE_RATE as f32;
        println!("kokoro: frio {cold:?}, quente {warm:?}, áudio {seconds:.2}s");
        assert!(seconds > 0.5, "áudio curto demais para a frase: {seconds}s");
    }

    #[test]
    fn a_voice_that_is_not_a_float32_array_is_refused_with_a_reason() {
        assert!(parse_npy(b"nao sou npy").is_err());
        let mut fake = b"\x93NUMPY\x01\x00".to_vec();
        let header = b"{'descr': '<f8', 'fortran_order': False, 'shape': (510, 1, 256), }";
        fake.extend_from_slice(&(header.len() as u16).to_le_bytes());
        fake.extend_from_slice(header);
        let err = parse_npy(&fake).unwrap_err();
        assert!(err.contains("float32"), "não explicou o motivo: {err}");
    }
}
