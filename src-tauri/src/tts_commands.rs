// SPEC: read-aloud (TTS-01, TTS-02, TTS-06, TTS-07, TTS-13, TTS-14,
//       TTS-20, TTS-21, TTS-22, TTS-23, TTS-24, TTS-26, TTS-31, TTS-32, TTS-35)

//! The boundary between the reader on screen and the voice on disk.
//!
//! Same split the rest of this codebase uses: everything that only needs a
//! `&Path` lives in `tts::`, and the `#[tauri::command]` around it does nothing
//! but resolve paths and forward. There is no Tauri integration runner in this
//! project, so what tests cover is the functions, never the commands.

use crate::providers::{PullProgress, PullStatus};
use crate::runtime::bundled;
use crate::tts::{speaker, timing, voices};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

/// One line of the voice catalog, as the screen shows it.
#[derive(Debug, Serialize)]
pub struct VoiceInfo {
    pub id: String,
    pub language: String,
    pub language_name: String,
    pub display_name: String,
    pub quality: String,
    /// `piper` or `kokoro`. The screen shows it, because the two cost very
    /// different things to install and the user is choosing between them.
    pub engine: String,
    /// `0` means "not measured yet" — the screen must say *unknown*, never
    /// *free*. Only the voice downloaded during the spike carries a real
    /// number, and inventing the rest would be inventing a fact the user reads.
    pub download_bytes: u64,
    pub installed: bool,
}

/// What the reader was told to use: the voice per language and the speed.
#[derive(Debug, Serialize)]
pub struct TtsSettings {
    pub voices: std::collections::BTreeMap<String, String>,
    pub speed: f32,
}

/// One sentence to speak, with where it sits on the page so the mark can move.
#[derive(Debug, Serialize)]
pub struct UtteranceInfo {
    pub block: usize,
    pub sentence: usize,
    pub text: String,
}

fn base_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config = crate::config::load_config(app)?
        .ok_or_else(|| "nenhuma pasta base configurada".to_string())?;
    if config.base_path.is_empty() {
        return Err("nenhuma pasta base configurada".to_string());
    }
    Ok(config.base_path_buf())
}

/// Progress out to the screen, on the same shape the model download already
/// uses: a channel drained into a Tauri event. Downloads do not return
/// progress, they emit it (AD-018).
fn forward_progress(app: &AppHandle, voice_id: String) -> tokio::sync::mpsc::Sender<PullProgress> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<PullProgress>(32);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app.emit(
                "voice-download-progress",
                VoiceDownloadProgress {
                    voice_id: voice_id.clone(),
                    progress,
                },
            );
        }
    });
    tx
}

#[derive(Clone, Serialize)]
struct VoiceDownloadProgress {
    voice_id: String,
    progress: PullProgress,
}

/// Where Piper writes its WAVs: the OS temp folder, never the user's book
/// folder (TTS-14). Nothing of this feature survives a restart.
fn temp_root() -> PathBuf {
    std::env::temp_dir()
}

/// Fetches piper's own `voices.json` and caches it, which is what turns the
/// six built-in entries into the full catalog: 176 voices in 57 languages.
///
/// Only ever called because the user asked for it — the app is local-first, and
/// a list that phoned home on every open would break that for no gain.
#[tauri::command]
pub async fn refresh_voice_catalog(app: AppHandle) -> Result<usize, String> {
    let base = base_path(&app)?;
    let dir = voices::voices_dir(&base);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let part = dir.join(format!("{}.part", voices::MANIFEST_FILE));
    let progress = forward_progress(&app, "catalog".to_string());
    crate::runtime::download::download_with_progress(&voices::manifest_url(), &part, progress)
        .await
        .map_err(|e| e.to_string())?;

    // Parsed before it is adopted: a half-written or changed manifest must not
    // replace a cache that works.
    let json = std::fs::read_to_string(&part).map_err(|e| e.to_string())?;
    let parsed = voices::parse_manifest(&json).inspect_err(|_| {
        let _ = std::fs::remove_file(&part);
    })?;
    std::fs::rename(&part, voices::manifest_path(&base)).map_err(|e| e.to_string())?;
    Ok(parsed.len())
}

#[tauri::command]
pub fn tts_settings(app: AppHandle) -> Result<TtsSettings, String> {
    let config = crate::config::load_config(&app)?.unwrap_or_default();
    Ok(TtsSettings {
        voices: config.tts_voices,
        speed: config.tts_speed,
    })
}

/// Pins a voice to a language (TTS-23). `None` unpins it.
#[tauri::command]
pub fn set_tts_voice(app: AppHandle, language: String, voice_id: Option<String>) -> Result<(), String> {
    let mut config = crate::config::load_config(&app)?.unwrap_or_default();
    match voice_id {
        Some(id) => config.tts_voices.insert(language, id),
        None => config.tts_voices.remove(&language),
    };
    // The live process was started with the old voice, so it has to go.
    speaker::shutdown();
    crate::config::save_config(&app, &config)
}

/// The reading speed (TTS-32), clamped to the range piper accepts.
#[tauri::command]
pub fn set_tts_speed(app: AppHandle, speed: f32) -> Result<f32, String> {
    let mut config = crate::config::load_config(&app)?.unwrap_or_default();
    config.tts_speed = speed.clamp(0.5, 2.0);
    // Speed is a process argument, so the child restarts on the next sentence.
    speaker::shutdown();
    crate::config::save_config(&app, &config)?;
    Ok(config.tts_speed)
}

#[tauri::command]
pub fn list_voices(app: AppHandle) -> Result<Vec<VoiceInfo>, String> {
    let base = base_path(&app)?;
    let have = voices::installed(&base);
    Ok(voices::catalog(&base)
        .into_iter()
        .map(|v| VoiceInfo {
            installed: have.iter().any(|id| *id == v.id),
            id: v.id,
            language: v.language,
            language_name: v.language_name,
            display_name: v.display_name,
            quality: v.quality,
            engine: v.engine.as_str().to_string(),
            download_bytes: v.model_bytes,
        })
        .collect())
}

/// Downloads a voice: model then config, each to a `.part` that is renamed only
/// after it is whole (TTS-26).
///
/// The rename is what makes `installed()` honest — a voice appears on the list
/// the instant both files are final, and never before.
#[tauri::command]
pub async fn download_voice(app: AppHandle, voice_id: String) -> Result<(), String> {
    let base = base_path(&app)?;
    let voice = voices::find(&base, &voice_id)
        .ok_or_else(|| format!("voz desconhecida: {voice_id}"))?;
    let dir = voices::voices_dir(&base);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Kokoro's two files are **shared by every kokoro voice**, so the second
    // voice downloads nothing. Piper's are the voice itself.
    let wanted = match voice.engine {
        voices::Engine::Kokoro => vec![
            (voice.model_url.clone(), voices::KOKORO_MODEL.to_string()),
            (voice.config_url.clone(), voices::KOKORO_VOICES.to_string()),
        ],
        voices::Engine::Piper => vec![
            (voice.model_url.clone(), format!("{voice_id}.onnx")),
            (voice.config_url.clone(), format!("{voice_id}.onnx.json")),
        ],
    };
    for (url, name) in wanted {
        if dir.join(&name).is_file() {
            // Already here: a second kokoro voice must not re-download 353 MB.
            continue;
        }
        let final_path = dir.join(&name);
        let part = dir.join(format!("{name}.part"));
        let progress = forward_progress(&app, voice_id.clone());
        let outcome = crate::runtime::download::download_with_progress(&url, &part, progress)
            .await
            .map_err(|e| e.to_string());
        if let Err(e) = outcome {
            // Nothing half-written survives: the `.part` is dropped and the
            // voice stays absent from `installed()` (TTS-26).
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        std::fs::rename(&part, &final_path).map_err(|e| {
            let _ = std::fs::remove_file(&part);
            format!("não foi possível concluir o download da voz: {e}")
        })?;
    }
    // The closing frame, for the same reason `download_model` emits one: without
    // it a finished download sits at whatever percentage the last chunk showed.
    let _ = app.emit(
        "voice-download-progress",
        VoiceDownloadProgress {
            voice_id: voice_id.clone(),
            progress: PullProgress {
                status: PullStatus::Success,
                downloaded_bytes: None,
                total_bytes: None,
                message: None,
            },
        },
    );
    Ok(())
}

#[tauri::command]
pub fn remove_voice(app: AppHandle, voice_id: String) -> Result<u64, String> {
    let base = base_path(&app)?;
    // Changing what is on disk under a live reader would leave it speaking from
    // a file that no longer exists.
    speaker::shutdown();
    // Removing one kokoro voice would mean removing the shared model, taking
    // the other nine with it. Only piper voices have files of their own.
    match voices::find(&base, &voice_id).map(|v| v.engine) {
        Some(voices::Engine::Kokoro) => Err(
            "as vozes do Kokoro dividem o mesmo modelo; remover uma removeria todas".to_string(),
        ),
        _ => voices::remove(&base, &voice_id).map_err(|e| e.to_string()),
    }
}

/// The sentences of a page, in reading order (TTS-02).
///
/// Cutting happens in Rust so the sentence boundary is the app's single one
/// (`pagination::sentence_starts`); the screen only receives the pieces and
/// where they belong.
#[tauri::command]
pub fn page_utterances(page_html: String) -> Vec<UtteranceInfo> {
    timing::utterances(&page_html)
        .into_iter()
        .map(|u| UtteranceInfo {
            block: u.block,
            sentence: u.sentence,
            text: u.text,
        })
        .collect()
}

/// Which installed voice answers for a language, or `None`.
///
/// `None` is not an error: it is the signal the screen uses to fall back to the
/// system voice (TTS-35), so a first run with nothing downloaded still reads.
#[tauri::command]
pub fn voice_for(app: AppHandle, language: String, chosen: Option<String>) -> Result<Option<String>, String> {
    let base = base_path(&app)?;
    let have = voices::installed(&base);
    // The stored choice wins over whatever the screen remembered: the settings
    // panel is where the user pinned it, and the reader must not disagree.
    let config = crate::config::load_config(&app)?.unwrap_or_default();
    let pinned = voices::pinned_for(&config.tts_voices, &language)
        .cloned()
        .or(chosen);
    Ok(voices::resolve(&have, &language, pinned.as_deref()).cloned())
}

/// Speaks one sentence and returns the WAV.
///
/// Raw bytes through `tauri::ipc::Response`, the same path `get_book_image`
/// uses — no base64. The duration travels **inside** the WAV header, so the
/// number and the audio cannot disagree.
///
/// **`(async)` is load-bearing.** A `#[tauri::command]` without it runs on the
/// main thread — the one that pumps the window — and synthesis is not fast
/// enough to hide there. Measured on this machine on 2026-09-08: Kokoro takes
/// 2,32 s on the first sentence (it loads a 325 MB graph) and 936 ms after
/// that. As a plain command those were 2,32 s of frozen window, which is what
/// "o Kokoro trava o programa" was. Piper's ~236 ms was small enough that
/// nobody noticed the same defect.
#[tauri::command(async)]
pub fn speak_sentence(
    app: AppHandle,
    voice_id: String,
    text: String,
) -> Result<tauri::ipc::Response, String> {
    let base = base_path(&app)?;
    let speed = crate::config::load_config(&app)?
        .map(|c| c.tts_speed)
        .unwrap_or(1.0);
    let entry = voices::find(&base, &voice_id)
        .ok_or_else(|| format!("voz desconhecida: {voice_id}"))?;

    let binary = piper_binary(&app)?;
    let (espeak_library, espeak_data) = espeak_paths(&app)?;
    let model = match entry.engine {
        voices::Engine::Kokoro => {
            // Kokoro runs on the ONNX Runtime, and `ort` picks the library from
            // `ORT_DYLIB_PATH`. Nothing on this path used to set it, so which
            // of the two `onnxruntime.dll` in the bundle got loaded was down to
            // the OS search order — piper's is from 2023.
            crate::rag::onnxruntime::ensure_dylib_blocking(&app)?;
            voices::kokoro_model_path(&base)
        }
        voices::Engine::Piper => voices::model_path(&base, &voice_id),
    };
    let request = speaker::Request {
        engine: entry.engine,
        binary: &binary,
        model: &model,
        voice_bundle: &voices::kokoro_voices_path(&base),
        espeak_library: &espeak_library,
        espeak_data: &espeak_data,
        // espeak wants `pt-br`, the catalog stores `pt_BR`.
        language: &entry.language.replace('_', "-").to_lowercase(),
        voice_id: &voice_id,
        temp_root: &temp_root(),
        speed,
    };
    let wav = speaker::speak_with(&request, &text)?;
    Ok(tauri::ipc::Response::new(wav))
}

/// Stops the reader and releases the child process.
#[tauri::command]
pub fn stop_speaking() {
    speaker::shutdown();
}

/// espeak-ng and its data, both inside the piper component - the same engine
/// piper phonemizes with, so nothing extra ships for Kokoro.
fn espeak_paths(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let root = bundled::resource_root(app)?;
    let name = if cfg!(windows) { "espeak-ng.dll" } else { "libespeak-ng.so" };
    let library = bundled::find_file(&root, name).ok_or_else(|| {
        format!("componente 'espeak-ng' não encontrado em {} — reinstale o ReadMe", root.display())
    })?;
    let data = library
        .parent()
        .map(|d| d.join("espeak-ng-data"))
        .ok_or("espeak-ng sem pasta de dados")?;
    Ok((library, data))
}

fn piper_binary(app: &AppHandle) -> Result<PathBuf, String> {
    let root = bundled::resource_root(app)?;
    let name = if cfg!(windows) { "piper.exe" } else { "piper" };
    bundled::find_file(&root, name).ok_or_else(|| {
        format!(
            "componente 'piper' não encontrado em {} — reinstale o ReadMe",
            root.display()
        )
    })
}
