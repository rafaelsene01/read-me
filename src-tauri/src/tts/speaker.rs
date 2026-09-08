// SPEC: read-aloud (TTS-01, TTS-02, TTS-06, TTS-13, TTS-32)

//! Keeping one `piper.exe` alive and turning a sentence into a WAV.
//!
//! **The shape of this file came from a measurement, not from taste.** The
//! archived Piper binary was run on this machine on 2026-09-07:
//!
//! | | |
//! |---|---|
//! | one sentence, cold process | 593 ms |
//! | five sentences, one process | 1.536 ms |
//! | marginal cost per sentence | ~236 ms |
//!
//! So the model load is ~2,5x the useful work of a sentence, and paying it per
//! sentence would make continuous reading stutter. The process stays alive and
//! is fed one line at a time on stdin - which the binary supports natively: its
//! own `--help` describes `--json-input` as *"stdin input is lines of JSON
//! **instead of plain text**"*, and plain text lines is therefore the default.
//! Confirmed by running it: five lines in, five WAVs out, model loaded once.
//!
//! It is not an HTTP sidecar, so it does not reuse `runtime::process::spawn` -
//! that function waits on a health endpoint Piper does not have. What it does
//! reuse is the part that matters for correctness on Windows:
//! `configure_command` (no console window, SIDE-03) and the Job Object, so a
//! crash of the app cannot leave the child running.

use super::kokoro;
use super::voices::Engine;
use crate::runtime::job::JobState;
use crate::runtime::process::configure_command;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

/// The one live Piper, and which voice it was started with.
///
/// ponytail: a global, single process, guarded by a `Mutex` - the same shape
/// `rag::pdfium`'s `EXTRACTING` lock uses, and for the same reason. There is one
/// screen and one pair of speakers, so two simultaneous readings are not a
/// product state. Per-voice pooling is the upgrade if that ever changes.
static PIPER: Mutex<Option<Running>> = Mutex::new(None);

/// The job object the child is put under, created once and held for the life of
/// the process.
///
/// It has to outlive the child: `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` kills
/// everything in the job when the *last handle closes*, so a `JobState` built
/// inline and dropped at the end of the statement terminates piper before it
/// can answer the first sentence. That was the actual defect - the reader
/// failed with "encerrou sem responder" on every play - fixed on 2026-09-08.
static PIPER_JOB: std::sync::OnceLock<JobState> = std::sync::OnceLock::new();

struct Running {
    voice: PathBuf,
    /// `--length_scale`: how long each phoneme lasts. Above 1 is slower.
    speed: f32,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    output_dir: PathBuf,
}

impl Running {
    fn kill(&mut self) {
        // Both deaths, deliberately: the explicit kill covers the ordinary
        // close, the Job Object covers the forced one. `AGENTS.md` says not to
        // remove either.
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.output_dir);
    }
}

/// The reading speed, as `--length_scale` — piper's own knob, confirmed in its
/// `--help`. It is a **process argument**, so changing it restarts the child;
/// at 593 ms that is cheaper than any way of avoiding it, and it happens when
/// the user moves a slider, not while reading.
///
/// Inverted on the way in because the two run opposite: a reader asks for
/// *faster*, and piper wants a *smaller* phoneme length.
pub fn length_scale(speed: f32) -> f32 {
    (1.0 / speed.clamp(0.5, 2.0)).clamp(0.5, 2.0)
}

/// Everything a voice needs to be spoken, whichever engine owns it.
///
/// One struct instead of eight arguments: the two engines need different files,
/// and threading both sets through every caller is how a wrong path reaches the
/// wrong engine.
pub struct Request<'a> {
    pub engine: Engine,
    /// `piper.exe` for Piper; unused by Kokoro.
    pub binary: &'a Path,
    /// The `.onnx` of the voice (Piper) or of the model (Kokoro).
    pub model: &'a Path,
    /// Kokoro's voice bundle; unused by Piper.
    pub voice_bundle: &'a Path,
    pub espeak_library: &'a Path,
    pub espeak_data: &'a Path,
    /// espeak's voice name: `pt-br`, `en-us`.
    pub language: &'a str,
    /// The voice id inside the bundle. Kokoro only.
    pub voice_id: &'a str,
    pub temp_root: &'a Path,
    pub speed: f32,
}

/// Speaks one sentence, returning WAV bytes - the same shape from either
/// engine, so nothing above this line knows which one spoke.
pub fn speak_with(request: &Request, sentence: &str) -> Result<Vec<u8>, String> {
    match request.engine {
        Engine::Piper => speak(
            request.binary,
            request.model,
            request.temp_root,
            sentence,
            request.speed,
        ),
        Engine::Kokoro => {
            let samples = kokoro::speak(
                request.model,
                request.voice_bundle,
                request.espeak_library,
                request.espeak_data,
                request.language,
                request.voice_id,
                sentence,
                request.speed,
            )?;
            Ok(kokoro::to_wav(&samples))
        }
    }
}

/// Speaks one sentence with Piper, returning the WAV bytes.
///
/// Starts Piper on the first call, and again whenever the voice or the speed
/// changes; every other call is one line written and one path read back.
pub fn speak(
    binary: &Path,
    voice: &Path,
    temp_root: &Path,
    sentence: &str,
    speed: f32,
) -> Result<Vec<u8>, String> {
    let mut guard = PIPER.lock().unwrap_or_else(|e| e.into_inner());

    let scale = length_scale(speed);
    let needs_restart = match guard.as_ref() {
        // A float compared exactly on purpose: the value comes from the same
        // stored setting every time, so it is either the same number or a new
        // one the user chose.
        Some(running) => running.voice != voice || running.speed != scale,
        None => true,
    };
    if needs_restart {
        if let Some(mut old) = guard.take() {
            old.kill();
        }
        *guard = Some(start(binary, voice, temp_root, scale)?);
    }

    let running = guard.as_mut().expect("just started");
    // One line in. A newline inside the sentence would be read as a second
    // request and desynchronise every reply after it, so it is flattened here -
    // the sentence splitter never produces one, and this is the guard that
    // makes that a property instead of a hope.
    let line: String = sentence
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    writeln!(running.stdin, "{}", line.trim()).map_err(|e| broken(e))?;
    running.stdin.flush().map_err(|e| broken(e))?;

    // One path out. Piper prints the file it wrote, one line per request.
    let mut reply = String::new();
    running.stdout.read_line(&mut reply).map_err(|e| broken(e))?;
    let path = reply.trim();
    if path.is_empty() {
        // Naming the exit code is the difference between a five-minute fix and
        // an afternoon: with stderr silenced, this line is the only thing that
        // says whether the child crashed, was killed, or simply stopped.
        let status = match running.child.try_wait() {
            Ok(Some(code)) => format!(" (saiu com {code})"),
            Ok(None) => " (ainda vivo)".to_string(),
            Err(e) => format!(" (estado desconhecido: {e})"),
        };
        if let Some(mut dead) = guard.take() {
            dead.kill();
        }
        return Err(format!("o leitor de voz encerrou sem responder{status}"));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| format!("não foi possível ler o áudio gerado: {e}"))?;
    // The audio is temporary by design (TTS-14): it is read once, handed to the
    // screen, and the file goes away. Nothing of this feature survives a
    // restart, and nothing of it lands in the user's book folder.
    let _ = std::fs::remove_file(path);
    Ok(bytes)
}

/// Stops the reader. Called when the app closes and when the voice changes.
pub fn shutdown() {
    let mut guard = PIPER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut running) = guard.take() {
        running.kill();
    }
}

fn broken(e: std::io::Error) -> String {
    format!("o leitor de voz parou de responder: {e}")
}

fn start(binary: &Path, voice: &Path, temp_root: &Path, scale: f32) -> Result<Running, String> {
    if !binary.exists() {
        return Err(format!(
            "componente 'piper' não encontrado em {} — reinstale o ReadMe",
            binary.display()
        ));
    }
    if !voice.exists() {
        return Err(format!(
            "a voz {} não está instalada; baixe uma voz para ouvir",
            voice.display()
        ));
    }
    let output_dir = temp_root.join(format!("readme-tts-{}", std::process::id()));
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("não foi possível preparar a pasta de áudio: {e}"))?;

    let mut cmd = Command::new(binary);
    cmd.arg("--model")
        .arg(voice)
        .arg("--output_dir")
        .arg(&output_dir)
        .arg("--length_scale")
        .arg(format!("{scale}"))
        .arg("--quiet")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // No console window on Windows. Without it `piper.exe` opens the black
    // terminal SIDE-03 exists to prevent.
    configure_command(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("não foi possível iniciar o leitor de voz: {e}"))?;
    PIPER_JOB.get_or_init(JobState::create).assign(&child);

    let stdin = child.stdin.take().ok_or("o leitor de voz não aceitou entrada")?;
    let stdout = child.stdout.take().ok_or("o leitor de voz não devolveu saída")?;
    Ok(Running {
        voice: voice.to_path_buf(),
        speed: scale,
        child,
        stdin,
        stdout: BufReader::new(stdout),
        output_dir,
    })
}

#[cfg(test)]
mod tests {
    //! ⚠️ O que está abaixo NÃO prova que a leitura em voz alta funciona.
    //!
    //! Não há como exercitar o `piper.exe` na suíte padrão: ele é um binário do
    //! bundle, resolvido por `AppHandle`, e o teste que o usasse dependeria de
    //! uma voz de 63 MB. O que estes testes cobrem é o único comportamento
    //! afirmável sem ele — as duas recusas antes de qualquer processo nascer.
    //!
    //! O caminho feliz é UAT, e a medição que sustenta o desenho está no
    //! cabeçalho deste arquivo, feita à mão em 2026-09-07.

    use super::*;

    /// The regression for the defect that made every play fail.
    ///
    /// `job.rs` proves the other half — closing the job kills what is inside
    /// it. That is why the job here must NOT be a temporary: written as
    /// `JobState::create().assign(&child)`, the handle closes at the end of the
    /// statement and the kernel kills piper before it prints its first path.
    /// This test spawns a stand-in child exactly the way `start` does and
    /// asserts it is still breathing afterwards.
    #[test]
    #[cfg(windows)]
    fn the_job_outlives_the_statement_that_assigns_the_child() {
        let mut child = Command::new("cmd")
            .args(["/c", "ping", "-n", "120", "127.0.0.1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn a stand-in child");

        PIPER_JOB.get_or_init(JobState::create).assign(&child);
        std::thread::sleep(std::time::Duration::from_millis(300));

        let alive = child.try_wait().expect("try_wait").is_none();
        let _ = child.kill();
        let _ = child.wait();
        assert!(alive, "the child was killed by its own job object");
    }

    #[test]
    fn asking_for_a_faster_reading_shortens_the_phonemes() {
        // As duas escalas correm em sentidos opostos, e trocá-las faria o
        // controle de velocidade andar para trás — o tipo de defeito que
        // ninguém percebe lendo o código, só ouvindo.
        assert_eq!(length_scale(1.0), 1.0);
        assert!(length_scale(2.0) < 1.0, "mais rápido tem de encurtar o fonema");
        assert!(length_scale(0.5) > 1.0, "mais lento tem de alongar o fonema");
        // E o pedido absurdo é limitado em vez de virar silêncio ou eternidade.
        assert_eq!(length_scale(50.0), 0.5);
        assert_eq!(length_scale(0.01), 2.0);
    }

    #[test]
    fn a_missing_binary_names_the_component_and_the_folder() {
        // TTS-13: a mesma forma que `bundled::missing` usa, para que um relato
        // de bug distinga "instalação incompleta" de "app confuso".
        let dir = std::env::temp_dir().join(format!("readme-tts-t1-{}", std::process::id()));
        let err = speak(
            &dir.join("nao-existe-piper.exe"),
            &dir.join("voz.onnx"),
            &dir,
            "olá",
            1.0,
        )
        .unwrap_err();

        assert!(err.contains("piper"), "não nomeou o componente: {err}");
        assert!(err.contains("reinstale"), "não disse o que fazer: {err}");
    }

    #[test]
    fn a_missing_voice_names_the_action_instead_of_failing_blank() {
        // TTS-24. O binário existe (este próprio executável de teste serve como
        // arquivo qualquer), a voz não.
        let dir = std::env::temp_dir().join(format!("readme-tts-t2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake_binary = dir.join("piper.exe");
        std::fs::write(&fake_binary, b"nao e um binario de verdade").unwrap();

        let err = speak(&fake_binary, &dir.join("ausente.onnx"), &dir, "olá", 1.0).unwrap_err();

        assert!(err.contains("baixe uma voz"), "não nomeou a ação: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
