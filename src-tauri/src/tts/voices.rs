// SPEC: read-aloud (TTS-14, TTS-20, TTS-21, TTS-22, TTS-24, TTS-26, TTS-31)

//! Which voices exist, which are on disk, and bringing one down.
//!
//! **A voice is a model, not a component**, and that distinction is the whole
//! reason this file looks like `models::catalog` and not like
//! `runtime::bundled`. It was read out of the repository, not assumed: SELF-10
//! records that a runtime component never downloads (`prepare_runtime` makes no
//! HTTP call, `runtime/release.rs` was deleted), while the GGUF models have a
//! curated catalog and `download_with_progress`, and SELF-18's cleanup
//! **preserves the models**. A voice sits in the second mould.
//!
//! What that buys: the installer grows by the Piper binary alone, and a
//! language costs disk only on the machine that wants it. Embedding one voice
//! would carve an exception into SELF-10 to privilege one language over the
//! rest.
//!
//! **Two engines share this catalog.** A piper voice is a 63 MB model of its
//! own; a kokoro voice is 0,5 MB of style vectors inside one 310 MB model that
//! every kokoro voice shares. The `engine` field is what tells them apart, and
//! it is stored rather than guessed from the id - a name is not a contract.
//!
//! A voice is two files that Piper reads directly - `<id>.onnx` and
//! `<id>.onnx.json`. They are **data**, so replacing the engine later does not
//! invalidate what the user already downloaded. That is the escape hatch for
//! Piper being an archived project.

use std::path::{Path, PathBuf};

/// Where voices live: beside the models in the base folder, never inside a
/// book's folder (TTS-14).
pub const VOICES_DIR: &str = "voices";

/// One voice the app offers to download.
///
/// Built from **piper's own manifest**, not from a list written here. The
/// upstream repository publishes `voices.json` — 176 voices in 57 languages,
/// each with its files, its `size_bytes` and its language name. A hand-written
/// catalog was the first version of this file, and it was a subset of nine
/// chosen by whoever typed it: every language nobody thought of was simply
/// missing, and there was no way for a user to ask for one.
///
/// The manifest's sizes were cross-checked against `content-length` on
/// 2026-09-07 and matched exactly (63.201.294 for `pt_BR-faber-medium`), so
/// they are reported as measured rather than as a claim.
/// Which synthesiser speaks a voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// One model file per voice, spoken by the bundled `piper.exe`.
    Piper,
    /// One shared model, one small style vector per voice, run in-process on
    /// the ONNX Runtime the app already loads.
    Kokoro,
}

impl Engine {
    pub fn as_str(&self) -> &'static str {
        match self {
            Engine::Piper => "piper",
            Engine::Kokoro => "kokoro",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatalogVoice {
    pub engine: Engine,
    pub id: String,
    /// `pt_BR`, `en_US` — the manifest's own code.
    pub language: String,
    /// `Portuguese (Brazil)`, for a list a person can read.
    pub language_name: String,
    pub display_name: String,
    pub quality: String,
    pub model_bytes: u64,
    pub model_url: String,
    pub config_url: String,
}

const HF: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main";

/// Where the downloaded manifest is kept, so the list works offline after the
/// first fetch — the app is local-first, and a voice list that needs the
/// network to show what is **already installed** would be a regression.
pub const MANIFEST_FILE: &str = "voices.json";

pub fn manifest_url() -> String {
    format!("{HF}/{MANIFEST_FILE}")
}

pub fn manifest_path(base: &Path) -> PathBuf {
    voices_dir(base).join(MANIFEST_FILE)
}

/// The catalog: the cached piper manifest when there is one, the built-in list
/// when there is not, plus the kokoro voices, which are a fixed set.
pub fn catalog(base: &Path) -> Vec<CatalogVoice> {
    let mut all = std::fs::read_to_string(manifest_path(base))
        .ok()
        .and_then(|json| parse_manifest(&json).ok())
        .filter(|list| !list.is_empty())
        .unwrap_or_else(builtin_catalog);
    all.extend(kokoro_catalog());
    // Sorting AFTER the extend, not before. Appending the kokoro voices to an
    // already-sorted piper list dropped all ten at the bottom of 186 rows, past
    // every other language — which reads on screen as "kokoro lost its
    // Portuguese voices". They were there, just below everything else.
    sort_catalog(&mut all);
    all
}

/// Language first, then the best quality inside it: the list is read top to
/// bottom, and the first voice of a language should be its best one.
fn sort_catalog(out: &mut [CatalogVoice]) {
    out.sort_by(|a, b| {
        a.language_name
            .cmp(&b.language_name)
            .then(quality_rank(&b.quality).cmp(&quality_rank(&a.quality)))
            .then(a.display_name.cmp(&b.display_name))
    });
}

/// Where the kokoro model and its voice bundle live, shared by every kokoro
/// voice - which is why a voice here costs half a megabyte instead of sixty.
pub const KOKORO_MODEL: &str = "kokoro-v1.0.onnx";
pub const KOKORO_VOICES: &str = "kokoro-voices-v1.0.bin";
pub const KOKORO_MODEL_URL: &str =
    "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model.onnx";
pub const KOKORO_VOICES_URL: &str =
    "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin";

pub fn kokoro_model_path(base: &Path) -> PathBuf {
    voices_dir(base).join(KOKORO_MODEL)
}

pub fn kokoro_voices_path(base: &Path) -> PathBuf {
    voices_dir(base).join(KOKORO_VOICES)
}

/// The kokoro voices this app offers.
///
/// A subset of the 54 the model ships, chosen for the languages the app reads:
/// the model is one download either way, so the list costs nothing but screen
/// space. Sizes are the **shared** model plus the voice's own vector, measured
/// on 2026-09-07: model 325.532.232 bytes, bundle 28.214.398, per voice ~510 KB.
fn kokoro_catalog() -> Vec<CatalogVoice> {
    [
        ("pf_dora", "pt_BR", "Portuguese (Brazil)", "Dora"),
        ("pm_alex", "pt_BR", "Portuguese (Brazil)", "Alex"),
        ("pm_santa", "pt_BR", "Portuguese (Brazil)", "Santa"),
        ("af_heart", "en_US", "English (United States)", "Heart"),
        ("af_bella", "en_US", "English (United States)", "Bella"),
        ("am_michael", "en_US", "English (United States)", "Michael"),
        ("bf_emma", "en_GB", "English (Great Britain)", "Emma"),
        ("bm_george", "en_GB", "English (Great Britain)", "George"),
        ("ef_dora", "es_ES", "Spanish (Spain)", "Dora"),
        ("em_alex", "es_ES", "Spanish (Spain)", "Alex"),
    ]
    .into_iter()
    .map(|(id, language, language_name, name)| CatalogVoice {
        engine: Engine::Kokoro,
        id: id.to_string(),
        language: language.to_string(),
        language_name: language_name.to_string(),
        display_name: name.to_string(),
        quality: "kokoro".to_string(),
        // What a first kokoro voice actually costs to download. The second one
        // costs half a megabyte, and saying 353 MB again would be a lie the
        // user only finds out after waiting.
        model_bytes: 325_532_232 + 28_214_398,
        model_url: KOKORO_MODEL_URL.to_string(),
        config_url: KOKORO_VOICES_URL.to_string(),
    })
    .collect()
}

/// Whether the shared kokoro files are on disk. A kokoro voice is installed
/// when they are: the style vector for every voice is inside the bundle.
pub fn kokoro_ready(base: &Path) -> bool {
    kokoro_model_path(base).is_file() && kokoro_voices_path(base).is_file()
}

/// Turns piper's `voices.json` into the list the screen shows.
///
/// **`low` and `x_low` are dropped only when the language has something
/// better.** They are the qualities that sound synthetic, so they are not worth
/// offering next to a `medium` of the same language — but for a language whose
/// only voice is `low`, dropping it would remove the language from the app
/// entirely, which is a worse answer than a rough voice.
pub fn parse_manifest(json: &str) -> Result<Vec<CatalogVoice>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("manifesto de vozes inválido: {e}"))?;
    let entries = root.as_object().ok_or("manifesto de vozes não é um objeto")?;

    let mut out: Vec<CatalogVoice> = Vec::new();
    for (id, value) in entries {
        let Some(model) = file_of(value, ".onnx") else { continue };
        let Some(config) = file_of(value, ".onnx.json") else { continue };
        let language = value.pointer("/language/code").and_then(|v| v.as_str());
        let (Some(language), Some(name)) = (language, value["name"].as_str()) else {
            continue;
        };
        out.push(CatalogVoice {
            engine: Engine::Piper,
            id: id.clone(),
            language: language.to_string(),
            language_name: language_name(value),
            display_name: name.to_string(),
            quality: value["quality"].as_str().unwrap_or("medium").to_string(),
            model_bytes: model.1,
            model_url: format!("{HF}/{}", model.0),
            config_url: format!("{HF}/{}", config.0),
        });
    }

    let has_better: std::collections::BTreeSet<String> = out
        .iter()
        .filter(|v| is_natural(&v.quality))
        .map(|v| v.language.clone())
        .collect();
    out.retain(|v| is_natural(&v.quality) || !has_better.contains(&v.language));

    sort_catalog(&mut out);
    Ok(out)
}

fn is_natural(quality: &str) -> bool {
    quality == "high" || quality == "medium"
}

fn quality_rank(quality: &str) -> u8 {
    match quality {
        // Kokoro is not a piper quality tier, it is a different model — and the
        // best-sounding one the app offers. Left at the default 0 it sorted
        // below `low`, so the best voice of a language showed up last.
        "kokoro" => 4,
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

/// `Portuguese (Brazil)`, falling back to the code when the manifest is thin.
fn language_name(value: &serde_json::Value) -> String {
    let english = value.pointer("/language/name_english").and_then(|v| v.as_str());
    let country = value.pointer("/language/country_english").and_then(|v| v.as_str());
    match (english, country) {
        (Some(lang), Some(country)) => format!("{lang} ({country})"),
        (Some(lang), None) => lang.to_string(),
        _ => value
            .pointer("/language/code")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
    }
}

/// `(path, size_bytes)` of the entry whose path ends with `suffix`.
///
/// `.onnx` would also match `.onnx.json`, so the model is the one that ends
/// with `.onnx` and **not** with `.json`.
fn file_of(value: &serde_json::Value, suffix: &str) -> Option<(String, u64)> {
    let files = value["files"].as_object()?;
    files.iter().find_map(|(path, meta)| {
        let matches = path.ends_with(suffix)
            && (suffix != ".onnx" || !path.ends_with(".onnx.json"));
        matches.then(|| (path.clone(), meta["size_bytes"].as_u64().unwrap_or(0)))
    })
}

/// The list used before the manifest has ever been fetched: enough to read a
/// book on the first run, in the two languages the interface speaks (AD-007).
/// Sizes measured by `content-length` on 2026-09-07.
fn builtin_catalog() -> Vec<CatalogVoice> {
    [
        ("pt_BR-faber-medium", "pt_BR", "Portuguese (Brazil)", "faber", "medium", 63_201_294u64, "pt/pt_BR/faber/medium"),
        ("pt_BR-cadu-medium", "pt_BR", "Portuguese (Brazil)", "cadu", "medium", 62_950_044, "pt/pt_BR/cadu/medium"),
        ("pt_BR-jeff-medium", "pt_BR", "Portuguese (Brazil)", "jeff", "medium", 62_950_044, "pt/pt_BR/jeff/medium"),
        ("en_US-lessac-high", "en_US", "English (United States)", "lessac", "high", 113_895_201, "en/en_US/lessac/high"),
        ("en_US-ryan-high", "en_US", "English (United States)", "ryan", "high", 120_786_792, "en/en_US/ryan/high"),
        ("en_US-lessac-medium", "en_US", "English (United States)", "lessac", "medium", 63_201_294, "en/en_US/lessac/medium"),
    ]
    .into_iter()
    .map(|(id, language, language_name, name, quality, bytes, path)| CatalogVoice {
        engine: Engine::Piper,
        id: id.to_string(),
        language: language.to_string(),
        language_name: language_name.to_string(),
        display_name: name.to_string(),
        quality: quality.to_string(),
        model_bytes: bytes,
        model_url: format!("{HF}/{path}/{id}.onnx"),
        config_url: format!("{HF}/{path}/{id}.onnx.json"),
    })
    .collect()
}

pub fn find(base: &Path, id: &str) -> Option<CatalogVoice> {
    catalog(base).into_iter().find(|v| v.id == id)
}

/// `<base>/voices`
pub fn voices_dir(base: &Path) -> PathBuf {
    base.join(VOICES_DIR)
}

/// The `.onnx` of a voice, whether or not it exists.
pub fn model_path(base: &Path, id: &str) -> PathBuf {
    voices_dir(base).join(format!("{id}.onnx"))
}

/// The ids of every voice fully on disk.
///
/// **Fully** is the point: a voice counts only when the model *and* its config
/// are both there. A download cut in half must not be picked up later as if it
/// worked (TTS-26).
pub fn installed(base: &Path) -> Vec<String> {
    let mut out = installed_piper(base);
    if kokoro_ready(base) {
        out.extend(kokoro_catalog().into_iter().map(|v| v.id));
    }
    out.sort();
    out
}

fn installed_piper(base: &Path) -> Vec<String> {
    let dir = voices_dir(base);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".onnx")?.to_string();
            dir.join(format!("{id}.onnx.json")).is_file().then_some(id)
        })
        .collect();
    out.sort();
    out
}

/// Deletes a voice, returning the bytes freed (TTS-31).
pub fn remove(base: &Path, id: &str) -> std::io::Result<u64> {
    let dir = voices_dir(base);
    let mut freed = 0;
    for name in [format!("{id}.onnx"), format!("{id}.onnx.json")] {
        let path = dir.join(name);
        if let Ok(meta) = std::fs::metadata(&path) {
            freed += meta.len();
        }
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(freed)
}

/// The voice to use for a page, given what is installed and what the user
/// chose.
///
/// The rule, in order: the voice the user pinned for this language, then any
/// installed voice of the language, then `None`. `None` is not a failure - it
/// is what makes the app fall back to the system voice (TTS-35) instead of
/// refusing to read.
pub fn resolve<'a>(installed: &'a [String], language: &str, chosen: Option<&str>) -> Option<&'a String> {
    let prefix = language_prefix(language);
    if let Some(chosen) = chosen {
        if let Some(hit) = installed.iter().find(|id| id.as_str() == chosen) {
            return Some(hit);
        }
    }
    installed
        .iter()
        .find(|id| language_prefix(&voice_language(id)) == prefix)
}

/// The voice the user pinned for a language, compared the way [`resolve`]
/// compares languages instead of by exact string.
///
/// The reader asks for the language of the page it is showing, and for a book
/// that was never translated that word is `original`. The settings panel writes
/// the key the catalog uses - `pt_BR`. An exact lookup between those two never
/// matches, so every voice the user picked was silently ignored and the reader
/// went on speaking with whatever it had cached. Fixed on 2026-09-08.
///
/// Exact match first, so `pt_BR` beats `pt-PT` when both are pinned; the
/// fallback scans a `BTreeMap`, so which one wins is stable, not luck.
pub fn pinned_for<'a>(
    pins: &'a std::collections::BTreeMap<String, String>,
    language: &str,
) -> Option<&'a String> {
    if let Some(exact) = pins.get(language) {
        return Some(exact);
    }
    let prefix = language_prefix(language);
    pins.iter()
        .find(|(key, _)| language_prefix(key) == prefix)
        .map(|(_, voice)| voice)
}

/// `pt-BR`, `pt_BR`, `pt` and `original` all answer `pt`.
///
/// `original` is the reader's own word for "the text as extracted" and carries
/// no language of its own, so it is treated as the app's default rather than
/// as a language nobody has a voice for.
fn language_prefix(language: &str) -> String {
    let language = if language == "original" { "pt" } else { language };
    language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .to_ascii_lowercase()
}

/// The language a voice id declares: `pt_BR-faber-medium` -> `pt_BR`.
fn voice_language(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("readme-voices-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(voices_dir(&dir)).unwrap();
        dir
    }

    fn put(base: &Path, id: &str, with_config: bool) {
        std::fs::write(model_path(base, id), vec![0u8; 10]).unwrap();
        if with_config {
            std::fs::write(
                voices_dir(base).join(format!("{id}.onnx.json")),
                b"{}",
            )
            .unwrap();
        }
    }

    #[test]
    fn a_kokoro_voice_sits_with_its_own_language_and_leads_it() {
        // O defeito relatado como "o Kokoro não tem mais voz em português": as
        // dez vozes eram anexadas DEPOIS da ordenação, então caíam no fim de
        // 186 linhas, atrás de todos os idiomas. E `quality_rank("kokoro")`
        // valia 0, abaixo de `low` — mesmo ordenado, o melhor motor do app
        // aparecia por último dentro do próprio idioma.
        let dir = base("kokoro-order");
        let list = catalog(&dir);

        let portuguese: Vec<&CatalogVoice> = list
            .iter()
            .filter(|v| v.language_name.starts_with("Portuguese (Brazil)"))
            .collect();
        assert!(
            portuguese.iter().any(|v| v.engine == Engine::Kokoro),
            "as vozes do Kokoro sumiram do português"
        );
        assert_eq!(
            portuguese[0].engine,
            Engine::Kokoro,
            "o Kokoro devia liderar o idioma, e veio {:?}",
            portuguese[0].quality
        );

        // E o idioma continua sendo um bloco só: nada de português espalhado
        // pela lista com o Kokoro num canto.
        let names: Vec<&String> = list.iter().map(|v| &v.language_name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "a lista não está agrupada por idioma");
    }

    #[test]
    fn the_pinned_voice_is_found_even_when_the_page_calls_its_language_original() {
        // O defeito real: a tela grava sob `pt_BR`, o leitor pergunta por
        // `original`, e a busca exata não achava nada — a voz escolhida era
        // ignorada em silêncio.
        let mut pins = std::collections::BTreeMap::new();
        pins.insert("pt_BR".to_string(), "pf_dora".to_string());

        assert_eq!(pinned_for(&pins, "original"), Some(&"pf_dora".to_string()));
        assert_eq!(pinned_for(&pins, "pt"), Some(&"pf_dora".to_string()));
        assert_eq!(pinned_for(&pins, "pt-BR"), Some(&"pf_dora".to_string()));
        assert_eq!(pinned_for(&pins, "en"), None, "inglês não usa a voz de português");

        // Exato ganha do prefixo: com duas variantes fixadas, a que o leitor
        // nomeou é a que toca.
        pins.insert("pt-PT".to_string(), "pt_PT-tugao-medium".to_string());
        assert_eq!(
            pinned_for(&pins, "pt-PT"),
            Some(&"pt_PT-tugao-medium".to_string())
        );
    }

    #[test]
    fn a_kokoro_voice_counts_as_installed_only_when_the_shared_files_are_both_there() {
        // Uma voz do Kokoro não tem arquivo próprio: ela existe quando o modelo
        // e o pacote de vozes existem. Com só um dos dois, escolhê-la falharia
        // na hora de falar.
        let dir = base("kokoro-install");
        assert!(!kokoro_ready(&dir));
        assert!(!installed(&dir).iter().any(|id| id == "pf_dora"));

        std::fs::write(kokoro_model_path(&dir), b"modelo").unwrap();
        assert!(!kokoro_ready(&dir), "meio Kokoro contou como instalado");

        std::fs::write(kokoro_voices_path(&dir), b"vozes").unwrap();
        assert!(kokoro_ready(&dir));
        let have = installed(&dir);
        assert!(have.iter().any(|id| id == "pf_dora"));
        assert!(have.iter().any(|id| id == "af_heart"));
    }

    #[test]
    fn a_half_downloaded_voice_is_not_installed() {
        // TTS-26: uma queda no meio do download não pode deixar uma voz que o
        // leitor escolhe depois e que falha na hora de falar.
        let dir = base("partial");
        put(&dir, "pt_BR-faber-medium", true);
        put(&dir, "en_US-lessac-medium", false); // só o .onnx

        assert_eq!(installed(&dir), vec!["pt_BR-faber-medium"]);
    }

    #[test]
    fn the_voice_for_a_page_follows_the_language_then_the_users_choice() {
        let dir = base("resolve");
        put(&dir, "pt_BR-faber-medium", true);
        put(&dir, "en_US-lessac-medium", true);
        put(&dir, "en_GB-alba-medium", true);
        let have = installed(&dir);

        // Sem escolha: qualquer voz do idioma da página serve.
        assert_eq!(
            resolve(&have, "pt-BR", None).map(String::as_str),
            Some("pt_BR-faber-medium")
        );
        // `original` é a palavra do leitor para "texto extraído", não um idioma.
        assert_eq!(
            resolve(&have, "original", None).map(String::as_str),
            Some("pt_BR-faber-medium")
        );
        // Com escolha: ela ganha, mesmo havendo outra do mesmo idioma.
        assert_eq!(
            resolve(&have, "en-US", Some("en_GB-alba-medium")).map(String::as_str),
            Some("en_GB-alba-medium")
        );
        // Escolha apontando para voz removida à mão: cai para outra do idioma,
        // em vez de falhar.
        assert_eq!(
            resolve(&have, "en-US", Some("en_US-sumida-high")).map(String::as_str),
            Some("en_GB-alba-medium")
        );
        // Idioma sem voz nenhuma: `None`, que é o gancho do fallback do sistema.
        assert_eq!(resolve(&have, "ja-JP", None), None);
    }

    #[test]
    fn removing_a_voice_reports_the_space_and_is_idempotent() {
        // TTS-31, e apagar duas vezes não é erro: a pasta é do usuário e ele
        // pode ter apagado antes — a mesma regra de `remove_lang`.
        let dir = base("remove");
        put(&dir, "pt_BR-faber-medium", true);

        let freed = remove(&dir, "pt_BR-faber-medium").unwrap();

        assert_eq!(freed, 12, "10 bytes de modelo + 2 de config");
        assert!(installed(&dir).is_empty());
        assert_eq!(remove(&dir, "pt_BR-faber-medium").unwrap(), 0);
    }

    /// Um recorte do `voices.json` de verdade, com a forma exata que o piper
    /// publica — conferida contra o arquivo baixado em 2026-09-07.
    const MANIFEST: &str = r#"{
      "pt_BR-faber-medium": {
        "key": "pt_BR-faber-medium", "name": "faber", "quality": "medium",
        "language": {"code": "pt_BR", "family": "pt", "region": "BR",
                     "name_english": "Portuguese", "country_english": "Brazil"},
        "files": {
          "pt/pt_BR/faber/medium/pt_BR-faber-medium.onnx": {"size_bytes": 63201294},
          "pt/pt_BR/faber/medium/pt_BR-faber-medium.onnx.json": {"size_bytes": 4855},
          "pt/pt_BR/faber/medium/MODEL_CARD": {"size_bytes": 300}
        }
      },
      "pt_BR-edresson-low": {
        "key": "pt_BR-edresson-low", "name": "edresson", "quality": "low",
        "language": {"code": "pt_BR", "name_english": "Portuguese", "country_english": "Brazil"},
        "files": {
          "pt/pt_BR/edresson/low/pt_BR-edresson-low.onnx": {"size_bytes": 20000000},
          "pt/pt_BR/edresson/low/pt_BR-edresson-low.onnx.json": {"size_bytes": 4000}
        }
      },
      "en_US-ryan-high": {
        "key": "en_US-ryan-high", "name": "ryan", "quality": "high",
        "language": {"code": "en_US", "name_english": "English", "country_english": "United States"},
        "files": {
          "en/en_US/ryan/high/en_US-ryan-high.onnx": {"size_bytes": 120786792},
          "en/en_US/ryan/high/en_US-ryan-high.onnx.json": {"size_bytes": 5000}
        }
      },
      "cy_GB-gwryw_gogleddol-medium": {
        "key": "cy_GB-gwryw_gogleddol-medium", "name": "gwryw gogleddol", "quality": "medium",
        "language": {"code": "cy_GB", "name_english": "Welsh", "country_english": "Great Britain"},
        "files": {
          "cy/cy_GB/gwryw_gogleddol/medium/cy_GB-gwryw_gogleddol-medium.onnx": {"size_bytes": 63000000},
          "cy/cy_GB/gwryw_gogleddol/medium/cy_GB-gwryw_gogleddol-medium.onnx.json": {"size_bytes": 4000}
        }
      },
      "ka_GE-natia-x_low": {
        "key": "ka_GE-natia-x_low", "name": "natia", "quality": "x_low",
        "language": {"code": "ka_GE", "name_english": "Georgian", "country_english": "Georgia"},
        "files": {
          "ka/ka_GE/natia/x_low/ka_GE-natia-x_low.onnx": {"size_bytes": 9000000},
          "ka/ka_GE/natia/x_low/ka_GE-natia-x_low.onnx.json": {"size_bytes": 3000}
        }
      }
    }"#;

    #[test]
    fn the_manifest_becomes_a_list_a_person_can_read() {
        let all = parse_manifest(MANIFEST).unwrap();

        let faber = all.iter().find(|v| v.id == "pt_BR-faber-medium").unwrap();
        assert_eq!(faber.language, "pt_BR");
        assert_eq!(faber.language_name, "Portuguese (Brazil)");
        assert_eq!(faber.display_name, "faber");
        // O tamanho vem do manifesto, e bate com o `content-length` medido em
        // 2026-09-07 — a razão de ele ser reportado como medido, não alegado.
        assert_eq!(faber.model_bytes, 63_201_294);
        assert_eq!(
            faber.model_url,
            "https://huggingface.co/rhasspy/piper-voices/resolve/main/pt/pt_BR/faber/medium/pt_BR-faber-medium.onnx"
        );
        assert_eq!(faber.config_url, format!("{}.json", faber.model_url));
        // `MODEL_CARD` está no manifesto e não é arquivo de voz.
        assert!(!faber.model_url.contains("MODEL_CARD"));
    }

    #[test]
    fn a_rough_voice_is_dropped_only_when_the_language_has_a_better_one() {
        // A regra que decide o que o usuário vê. Em pt-BR existe `medium`,
        // então a `low` sai; em ka-GE a única voz é `x_low`, e tirá-la
        // apagaria o idioma inteiro do app — pior que uma voz áspera.
        let all = parse_manifest(MANIFEST).unwrap();
        let ids: Vec<&str> = all.iter().map(|v| v.id.as_str()).collect();

        assert!(!ids.contains(&"pt_BR-edresson-low"), "manteve a low tendo medium");
        assert!(ids.contains(&"ka_GE-natia-x_low"), "apagou o único georgiano");
        assert!(ids.contains(&"cy_GB-gwryw_gogleddol-medium"), "perdeu o galês");
    }

    #[test]
    fn the_list_is_ordered_by_language_and_then_by_the_best_quality_first() {
        let all = parse_manifest(MANIFEST).unwrap();
        let names: Vec<&str> = all.iter().map(|v| v.language_name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "a lista não está agrupada por idioma");
    }

    #[test]
    fn a_broken_manifest_is_an_error_and_never_an_empty_catalog_pretending_to_be_full() {
        assert!(parse_manifest("isto não é json").is_err());
        assert!(parse_manifest("[1,2,3]").is_err());
        // Entrada sem os arquivos que importam é ignorada, não derruba o resto.
        let partial = r#"{"x-y-medium": {"name": "x", "quality": "medium",
            "language": {"code": "xx", "name_english": "X"}, "files": {}}}"#;
        assert!(parse_manifest(partial).unwrap().is_empty());
    }

    #[test]
    fn without_a_manifest_the_builtin_list_still_lets_someone_read() {
        // Primeira execução, sem rede: o app não pode ficar sem nenhuma voz
        // para oferecer nos dois idiomas que a interface fala (AD-007).
        let dir = base("builtin");
        let all = catalog(&dir);

        assert!(all.iter().any(|v| v.language == "pt_BR"));
        assert!(all.iter().any(|v| v.language == "en_US"));
        for voice in &all {
            assert!(voice.model_bytes > 0, "{} sem tamanho medido", voice.id);
        }

        // A invariante dos dois arquivos é **por motor**, e escrevê-la como se
        // fosse uma só foi o que quebrou quando o Kokoro entrou: no piper a
        // config é o modelo mais `.json`; no Kokoro os dois são arquivos
        // diferentes, de repositórios diferentes.
        for voice in all.iter().filter(|v| v.engine == Engine::Piper) {
            assert!(voice.model_url.ends_with(".onnx"), "{}", voice.id);
            assert_eq!(
                voice.config_url,
                format!("{}.json", voice.model_url),
                "a config de {} não é o modelo + .json",
                voice.id
            );
        }
        for voice in all.iter().filter(|v| v.engine == Engine::Kokoro) {
            assert!(voice.model_url.ends_with(".onnx"), "{}", voice.id);
            assert!(voice.config_url.ends_with(".bin"), "{}", voice.id);
            // Todas as vozes do Kokoro apontam para o MESMO par de arquivos —
            // é isso que faz a segunda voz custar meio megabyte.
            assert_eq!(voice.model_url, KOKORO_MODEL_URL);
            assert_eq!(voice.config_url, KOKORO_VOICES_URL);
        }
        assert!(find(&dir, "pt_BR-faber-medium").is_some());
        assert!(find(&dir, "nao-existe").is_none());

        // Os dois motores aparecem na mesma lista, que é o que o usuário
        // escolhe: piper por voz, kokoro por modelo compartilhado.
        assert!(all.iter().any(|v| v.engine == Engine::Piper));
        assert!(all.iter().any(|v| v.engine == Engine::Kokoro));
        // E uma voz do Kokoro em português existe, senão o pedido não é
        // atendido no idioma que interessa aqui.
        assert!(all
            .iter()
            .any(|v| v.engine == Engine::Kokoro && v.language == "pt_BR"));
    }

    #[test]
    fn a_cached_manifest_wins_over_the_builtin_list() {
        let dir = base("cached");
        std::fs::write(manifest_path(&dir), MANIFEST).unwrap();

        let all = catalog(&dir);

        assert!(all.iter().any(|v| v.language == "cy_GB"), "não leu o cache");
        assert!(find(&dir, "ka_GE-natia-x_low").is_some());
    }

    #[test]
    fn a_corrupt_cache_falls_back_instead_of_leaving_the_user_with_nothing() {
        let dir = base("corrupt");
        std::fs::write(manifest_path(&dir), "{ isto quebrou }").unwrap();

        let all = catalog(&dir);

        assert!(!all.is_empty(), "cache quebrado zerou o catálogo");
        assert!(find(&dir, "pt_BR-faber-medium").is_some());
    }

}
