// SPEC: book-reader (READ-12, READ-14, READ-20, READ-21, READ-22, READ-23, READ-30)

//! Translating a book, one paragraph per request and one page per checkpoint.
//!
//! The two loops have different units on purpose: the **page** is what gets
//! written and resumed from, the **paragraph** is what gets sent. A whole page
//! (~2.500 characters) handed to a small model makes it summarize, truncate or
//! start commenting - measured in T1 against Moby-Dick chapter 1.
//!
//! The design put this loop in the command because it needs an `AppHandle`.
//! It does not: the translator arrives as a closure, so everything here runs
//! against a temp folder with no Tauri at all - which is the only way the
//! resume and reassembly rules get a test in this project.

use super::{pagination, storage};
use crate::chat::cancellation::CancellationToken;
use crate::providers::llama_server::LlamaServerClient;
use crate::providers::ChatMessage;
use crate::runtime::store::ActiveModel;
use futures_util::StreamExt;
use std::path::Path;

/// The curated entry translation falls back to when no model is active.
/// Chosen by measurement in T1, not by reputation - see the design's
/// measurement table for the page, the candidates and the numbers: 12,53 s
/// and 56,4 tok/s on a 2.161-character page, and the only one of the three
/// measured that kept the proper nouns intact.
pub const DEFAULT_TRANSLATION_MODEL: &str = "gguf-qwen2.5-7b";

/// The whole selection rule: there is an active model, or the error names the
/// one to download. Forcing the default instead would restart the sidecar
/// twice per book (`set_active_model` reloads the runtime) for a difference
/// nobody measured.
pub fn select_model(active: Option<ActiveModel>) -> Result<ActiveModel, String> {
    active.ok_or_else(|| {
        format!("Nenhum modelo ativo para traduzir. Baixe ou ative {DEFAULT_TRANSLATION_MODEL}.")
    })
}

/// Asking for the target language is enough; the model works out the source on
/// its own, which is a language detector this app does not have to own.
pub fn prompt(language: &str, paragraph: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage::system(format!(
            "You are a translator. Translate the user's text to {language}. \
             Output only the translation, no commentary, no explanation. \
             The text is a single paragraph: reply with a single paragraph."
        )),
        ChatMessage::user(paragraph),
    ]
}

/// One request, accumulated instead of streamed to the screen: the reader
/// shows finished pages, so a half-arrived paragraph has nowhere to go.
/// Local sidecar only - there is no network call in this path (READ-20).
pub async fn translate_paragraph(
    client: &LlamaServerClient,
    model: &ActiveModel,
    language: &str,
    paragraph: &str,
) -> Result<String, String> {
    let mut stream = client
        .stream_chat(
            &model.name,
            prompt(language, paragraph),
            model.context_length,
        )
        .await
        .map_err(|e| e.to_string())?;

    let mut accumulated = String::new();
    while let Some(item) = stream.next().await {
        accumulated.push_str(&item.map_err(|e| e.to_string())?.delta);
    }
    Ok(accumulated)
}

/// Translates every page of `language` that has no file yet, lowest first.
///
/// `language` is a parameter and not a column: a book can be finished in `pt`
/// and half done in `en`, and both are true at the same time (READ-30).
/// `None` is "do not translate" and never reaches the model.
///
/// `progress` is called at the top of every page with the number of pages
/// already on disk for this language - the absolute count, so a resumed book
/// does not restart its bar at zero - and answers whether the book still
/// exists. Returning `false` stops the loop, the same guard the document
/// pipeline uses so a book deleted mid-run cannot recreate the folder that was
/// just removed.
///
/// Returns how many pages this run wrote.
pub async fn translate_book<T, Fut, P>(
    book_dir: &Path,
    language: Option<&str>,
    page_count: u32,
    cancelled: &CancellationToken,
    mut translate: T,
    mut progress: P,
) -> Result<u32, String>
where
    T: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
    P: FnMut(u32) -> bool,
{
    let Some(language) = language else {
        return Ok(0);
    };
    let lang = storage::lang_dir(book_dir, language).map_err(|e| e.to_string())?;
    let original = storage::lang_dir(book_dir, storage::ORIGINAL_DIR).map_err(|e| e.to_string())?;

    let mut written = 0;
    loop {
        // Two directory reads per page instead of one, deliberately: it keeps
        // `next_missing` the single definition of "which page is pending"
        // (READ-14), and a readdir next to a 12-second generation is noise.
        if !progress(storage::translated_pages(&lang).len() as u32) {
            break;
        }
        if cancelled.is_cancelled() {
            break;
        }
        let Some(page) = storage::next_missing(&lang, page_count) else {
            break;
        };

        let text = storage::read_page(&original, page).map_err(|e| e.to_string())?;
        let Some(translated) = translate_page(&text, cancelled, &mut translate).await? else {
            // Cancelled between paragraphs: nothing is written, so "the file
            // exists" keeps meaning "the page is complete".
            break;
        };

        std::fs::create_dir_all(&lang).map_err(|e| e.to_string())?;
        std::fs::write(storage::page_file(&lang, page), translated).map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(written)
}

/// One page: split, one request per paragraph, joined back with the blank line
/// between them (READ-23). `Ok(None)` means cancelled mid-page.
///
/// Paragraphs are trimmed before joining, so the file matches what
/// `storage::write_pages` leaves in `original/`: no trailing separator, and
/// the file on disk is exactly the page.
async fn translate_page<T, Fut>(
    text: &str,
    cancelled: &CancellationToken,
    translate: &mut T,
) -> Result<Option<String>, String>
where
    T: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let mut out: Vec<String> = Vec::new();
    // The same `split_paragraphs` pagination uses. One definition of
    // "paragraph", not two: the one that drifted would cut a paragraph in half
    // in exactly the case the per-paragraph request exists to avoid.
    for paragraph in pagination::split_paragraphs(text) {
        // Checked between paragraphs, not only between pages: on a long page
        // the user would otherwise wait for every remaining request.
        if cancelled.is_cancelled() {
            return Ok(None);
        }
        let translated = translate(paragraph.to_string()).await?;
        if translated.trim().is_empty() {
            // Writing the page without it would produce a book with a hole
            // nobody notices until they read it, and then the blame lands on
            // the translation instead of on this bug.
            return Err(format!(
                "o modelo devolveu um parágrafo vazio; a página não foi gravada: {:?}",
                paragraph.chars().take(60).collect::<String>()
            ));
        }
        out.push(translated.trim().to_string());
    }
    Ok(Some(out.join("\n\n")))
}

#[cfg(test)]
mod tests {
    //! ⚠️ **Nenhuma asserção aqui olha a qualidade da tradução**, e nenhum
    //! teste sobe o `llama-server` ou toca a rede: o tradutor é sempre um
    //! duble. O que estes testes provam é a seleção do modelo, a unidade de
    //! requisição, a remontagem e a persistência retomável. Que a saída do
    //! modelo seja uma tradução **boa** é medição (T1) e UAT (T13), não teste
    //! — sem o modelo no ar não há como afirmá-lo, e um teste que fingisse
    //! afirmá-lo seria pior que a ausência dele.

    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    /// Uma pasta de livro por teste, sempre sob `std::env::temp_dir()` e
    /// **nunca** vinda da configuração do app: a biblioteca real do usuário
    /// não pode ser alcançada por teste nenhum.
    fn book(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("readme-translate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `original/` com `pages` páginas de um parágrafo cada.
    fn with_original(dir: &Path, pages: u32) -> PathBuf {
        let original = storage::lang_dir(dir, storage::ORIGINAL_DIR).unwrap();
        let bodies: Vec<String> = (1..=pages).map(|i| format!("Page {i} text.")).collect();
        storage::write_pages(&original, &bodies).unwrap();
        original
    }

    /// O duble de tradutor: registra o que recebeu e devolve o texto marcado.
    /// É o único "modelo" que estes testes conhecem.
    #[allow(clippy::type_complexity)]
    fn recorder() -> (
        Rc<RefCell<Vec<String>>>,
        impl FnMut(String) -> std::future::Ready<Result<String, String>>,
    ) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = seen.clone();
        (seen, move |paragraph: String| {
            log.borrow_mut().push(paragraph.clone());
            std::future::ready(Ok(format!("[pt] {paragraph}")))
        })
    }

    fn always_alive(_done: u32) -> bool {
        true
    }

    #[test]
    fn next_untranslated_returns_the_lowest_page_without_a_translation() {
        // READ-14. O mecanismo de retomada é `storage::next_missing`, e a T6
        // apenas o dirige — por isso a asserção é sobre ele, e não sobre uma
        // segunda definição de "qual falta", que divergiria.
        let dir = book("next-lowest");
        let pt = storage::lang_dir(&dir, "pt").unwrap();
        storage::write_pages(&pt, &(1..=5).map(|i| format!("p{i}")).collect::<Vec<_>>()).unwrap();
        std::fs::remove_file(storage::page_file(&pt, 2)).unwrap();
        std::fs::remove_file(storage::page_file(&pt, 4)).unwrap();

        assert_eq!(storage::next_missing(&pt, 5), Some(2));
    }

    #[test]
    fn next_untranslated_returns_none_when_every_page_is_done() {
        let dir = book("next-none");
        let pt = storage::lang_dir(&dir, "pt").unwrap();
        storage::write_pages(&pt, &(1..=3).map(|i| format!("p{i}")).collect::<Vec<_>>()).unwrap();

        assert_eq!(storage::next_missing(&pt, 3), None);
    }

    #[tokio::test]
    async fn a_translated_page_leaves_the_original_file_untouched() {
        let dir = book("original-untouched");
        let original = with_original(&dir, 2);
        let before: Vec<Vec<u8>> = (1..=2)
            .map(|i| std::fs::read(storage::page_file(&original, i)).unwrap())
            .collect();

        let (_, mut translator) = recorder();
        let written = translate_book(
            &dir,
            Some("pt"),
            2,
            &CancellationToken::default(),
            &mut translator,
            always_alive,
        )
        .await
        .unwrap();

        assert_eq!(written, 2);
        for (i, bytes) in before.iter().enumerate() {
            assert_eq!(
                &std::fs::read(storage::page_file(&original, i as u32 + 1)).unwrap(),
                bytes,
                "original/{:04}.txt mudou ao traduzir",
                i + 1
            );
        }
        assert_eq!(
            storage::read_page(&storage::lang_dir(&dir, "pt").unwrap(), 1).unwrap(),
            "[pt] Page 1 text."
        );
    }

    #[tokio::test]
    async fn pages_translated_before_a_cancel_are_not_lost() {
        // READ-14: o que já saiu fica, e a rodada seguinte recomeça na
        // primeira página pendente em vez de refazer o livro.
        let dir = book("resume");
        with_original(&dir, 4);
        let pt = storage::lang_dir(&dir, "pt").unwrap();

        let cancelled = CancellationToken::default();
        let (seen, mut translator) = recorder();
        let stop = cancelled.clone();
        let written = translate_book(
            &dir,
            Some("pt"),
            4,
            &cancelled,
            &mut translator,
            move |done| {
                if done == 2 {
                    stop.cancel();
                }
                true
            },
        )
        .await
        .unwrap();

        assert_eq!(written, 2, "o cancelamento não parou entre páginas");
        assert_eq!(storage::translated_pages(&pt).len(), 2);
        assert_eq!(storage::next_missing(&pt, 4), Some(3));
        assert_eq!(seen.borrow().len(), 2, "pediu tradução depois de cancelar");

        // A retomada: mesmo idioma, token novo, e só o que faltava é pedido.
        let (again, mut translator) = recorder();
        let written = translate_book(
            &dir,
            Some("pt"),
            4,
            &CancellationToken::default(),
            &mut translator,
            always_alive,
        )
        .await
        .unwrap();

        assert_eq!(written, 2);
        assert_eq!(again.borrow().as_slice(), ["Page 3 text.", "Page 4 text."]);
        assert_eq!(storage::next_missing(&pt, 4), None);
    }

    #[tokio::test]
    async fn asking_for_no_translation_never_touches_the_model() {
        // O duble falha se for chamado, e o `progress` entra em pânico: "não
        // traduzir" é a opção pré-selecionada do diálogo, e ela não pode
        // custar um modelo nem uma requisição.
        //
        // ⚠️ Inconclusivo quanto ao comando: que `process_book` também não
        // chame `select_model` quando `language` é `None` não é provado aqui
        // — não há runner de integração Tauri. Lá isso é `language.map(...)`,
        // conferido por leitura.
        let dir = book("no-translation");
        with_original(&dir, 3);

        let written = translate_book(
            &dir,
            None,
            3,
            &CancellationToken::default(),
            |_: String| std::future::ready(Err::<String, String>("o modelo foi chamado".into())),
            |_| panic!("o laço rodou sem idioma"),
        )
        .await
        .unwrap();

        assert_eq!(written, 0);
        assert!(!storage::lang_dir(&dir, "pt").unwrap().exists());
    }

    #[test]
    fn the_default_translation_model_is_a_curated_catalog_id() {
        // READ-21: sem isto a oferta de download apontaria para o nada — o
        // diálogo passa este id adiante, e quem o resolve é o catálogo.
        assert!(
            crate::models::catalog::curated_models()
                .iter()
                .any(|m| m.id == DEFAULT_TRANSLATION_MODEL),
            "{DEFAULT_TRANSLATION_MODEL} não é id de CURATED_MODELS"
        );
    }

    #[test]
    fn translation_without_an_active_model_names_the_default_instead_of_failing_blank() {
        let err = select_model(None).unwrap_err();
        assert!(
            err.contains(DEFAULT_TRANSLATION_MODEL),
            "a mensagem não nomeia o modelo: {err}"
        );

        let active = ActiveModel {
            name: "outro.gguf".to_string(),
            path: "/models/outro.gguf".to_string(),
            context_length: Some(8192),
            gpu_layers: None,
        };
        assert_eq!(select_model(Some(active)).unwrap().name, "outro.gguf");
    }

    #[tokio::test]
    async fn each_request_carries_exactly_one_paragraph() {
        // READ-22. Uma página de 4 parágrafos são 4 requisições, e nenhuma
        // delas carrega a fronteira entre dois.
        let dir = book("one-paragraph");
        let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap();
        let page = (1..=4)
            .map(|i| format!("Paragraph {i}."))
            .collect::<Vec<_>>()
            .join("\n\n");
        storage::write_pages(&original, &[page]).unwrap();

        let (seen, mut translator) = recorder();
        translate_book(
            &dir,
            Some("pt"),
            1,
            &CancellationToken::default(),
            &mut translator,
            always_alive,
        )
        .await
        .unwrap();

        let seen = seen.borrow();
        assert_eq!(
            seen.len(),
            4,
            "não foi uma requisição por parágrafo: {seen:?}"
        );
        for request in seen.iter() {
            assert!(
                !request.contains("\n\n"),
                "a requisição levou mais de um parágrafo: {request:?}"
            );
        }
        assert_eq!(
            seen.as_slice(),
            [
                "Paragraph 1.",
                "Paragraph 2.",
                "Paragraph 3.",
                "Paragraph 4."
            ]
        );
    }

    #[tokio::test]
    async fn the_page_is_reassembled_with_its_paragraph_breaks() {
        // READ-23: 3 parágrafos traduzidos voltam como 3 parágrafos. Um bloco
        // só perderia a estrutura que a extração preservou.
        let dir = book("reassembly");
        let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap();
        storage::write_pages(&original, &["Um.\n\nDois.\n\nTrês.".to_string()]).unwrap();

        let (_, mut translator) = recorder();
        translate_book(
            &dir,
            Some("pt"),
            1,
            &CancellationToken::default(),
            &mut translator,
            always_alive,
        )
        .await
        .unwrap();

        let written = storage::read_page(&storage::lang_dir(&dir, "pt").unwrap(), 1).unwrap();
        assert_eq!(written, "[pt] Um.\n\n[pt] Dois.\n\n[pt] Três.");
        assert_eq!(
            pagination::split_paragraphs(&written).len(),
            3,
            "a página voltou como um bloco só: {written:?}"
        );
    }

    #[tokio::test]
    async fn an_empty_paragraph_translation_fails_the_page_instead_of_dropping_it() {
        let dir = book("empty-paragraph");
        let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap();
        storage::write_pages(&original, &["Um.\n\nDois.\n\nTrês.".to_string()]).unwrap();

        let calls = Rc::new(RefCell::new(0));
        let counter = calls.clone();
        let err = translate_book(
            &dir,
            Some("pt"),
            1,
            &CancellationToken::default(),
            move |p: String| {
                let mut n = counter.borrow_mut();
                *n += 1;
                // Espaço em branco, não string vazia: o buraco silencioso é
                // exatamente o que passa por "resposta" sem ser uma.
                let reply = if *n == 2 { "   ".to_string() } else { p };
                std::future::ready(Ok(reply))
            },
            always_alive,
        )
        .await
        .unwrap_err();

        assert!(err.contains("vazio"), "{err}");
        assert!(
            !storage::page_file(&storage::lang_dir(&dir, "pt").unwrap(), 1).exists(),
            "a página foi gravada com buraco"
        );
        assert_eq!(*calls.borrow(), 2, "continuou pedindo depois da falha");
    }

    #[tokio::test]
    async fn a_cancel_mid_page_leaves_the_page_untranslated() {
        // O cancelamento é conferido ENTRE parágrafos: numa página longa,
        // esperar os que faltam seria minutos depois do clique.
        let dir = book("cancel-mid-page");
        let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap();
        storage::write_pages(&original, &["Um.\n\nDois.\n\nTrês.\n\nQuatro.".to_string()]).unwrap();

        let cancelled = CancellationToken::default();
        let stop = cancelled.clone();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = seen.clone();
        let written = translate_book(
            &dir,
            Some("pt"),
            1,
            &cancelled,
            move |p: String| {
                log.borrow_mut().push(p.clone());
                stop.cancel();
                std::future::ready(Ok(p))
            },
            always_alive,
        )
        .await
        .unwrap();

        assert_eq!(written, 0);
        assert_eq!(seen.borrow().len(), 1, "não parou no parágrafo seguinte");
        let pt = storage::lang_dir(&dir, "pt").unwrap();
        assert!(
            !storage::page_file(&pt, 1).exists(),
            "meia página foi gravada"
        );
        assert_eq!(storage::next_missing(&pt, 1), Some(1));
    }
}
