// SPEC: book-reader (READ-02, READ-03, READ-08, READ-11, READ-12, READ-13, READ-14, READ-16,
//       READ-17, READ-20, READ-21, READ-26, READ-27, READ-28, READ-29, READ-30),
//       reading-history (HIST-02, HIST-04, HIST-05, HIST-06, HIST-07)

//! Turning an imported book into pages on disk.
//!
//! The work is split the same way `import_all` is split in
//! `library_commands`: everything that only needs a `&Connection` and a
//! `&Path` lives in `process_into_pages`, and the `#[tauri::command]` around
//! it does nothing but resolve those two and forward the events. There is no
//! Tauri integration runner in this project, so what is exercised by tests is
//! the function, never the command.

use crate::chat::cancellation::{CancellationRegistry, CancellationToken};
use crate::db::{require_conn, DbState};
use crate::rag::parsing::{extension_of, ParseError};
use crate::reader::{epub, pagination, storage, translate};
use crate::runtime::store::ActiveModel;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

/// Where a book sits in the reader pipeline. Same shape as `DocumentStatus`
/// (`rag/pipeline.rs`), serde renaming included, so the frontend reads
/// `book-status` exactly as it already reads `document-status` — a second
/// convention in the same codebase would only be a second thing to remember.
///
/// `translating` is deliberately absent: translating is work *per language*,
/// derived from the files in `<lang>/` against `page_count`. A single column
/// could describe only one language and would lie about the others.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BookStatus {
    Imported,
    Extracting,
    Paginating,
    Ready,
    Error,
}

impl BookStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BookStatus::Imported => "imported",
            BookStatus::Extracting => "extracting",
            BookStatus::Paginating => "paginating",
            BookStatus::Ready => "ready",
            BookStatus::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BookStatusEvent {
    pub id: String,
    pub status: BookStatus,
    /// `None` outside a translation. Present, it is the language being
    /// translated — a book can be ready in pt and half done in en, and one bar
    /// for both would lie about both. T6 is what fills it.
    pub language: Option<String>,
    pub done: u32,
    pub total: u32,
    pub error_message: Option<String>,
}

/// Writes the status to the row and announces it in the same call, so the two
/// can never disagree — the same pairing `set_status` does for documents.
///
/// `done` and `total` carry the page count and are equal here: extraction and
/// pagination are not divisible into pages, so this half of the flow has no
/// partial progress to report. Per-page counters arrive with the translation
/// loop (T6).
fn set_status(
    conn: &Connection,
    book_id: &str,
    status: BookStatus,
    error_message: Option<String>,
    pages: u32,
    emit: &mut dyn FnMut(BookStatusEvent),
) -> Result<(), String> {
    conn.execute(
        "UPDATE books SET status = ?1, error_message = ?2 WHERE id = ?3",
        params![status.as_str(), error_message, book_id],
    )
    .map_err(|e| e.to_string())?;
    emit(BookStatusEvent {
        id: book_id.to_string(),
        status,
        language: None,
        done: pages,
        total: pages,
        error_message,
    });
    Ok(())
}

/// Marks the book as failed and hands the message back so the caller can
/// return it. Three steps can fail after the row has already moved to
/// `extracting`, and each of them has to leave the same trail.
fn fail(
    conn: &Connection,
    book_id: &str,
    message: String,
    emit: &mut dyn FnMut(BookStatusEvent),
) -> String {
    let _ = set_status(
        conn,
        book_id,
        BookStatus::Error,
        Some(message.clone()),
        0,
        emit,
    );
    message
}

/// The book's file and its folder, refusing what the reader cannot open.
///
/// The refusal happens before any status is written: a `.mobi` is a book the
/// library accepts and the reader does not open (READ-03), not a book whose
/// processing failed, and nothing on disk is touched to find that out.
fn book_paths(
    conn: &Connection,
    library_dir: &Path,
    book_id: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let (filename, folder): (String, Option<String>) = conn
        .query_row(
            "SELECT filename, folder FROM books WHERE id = ?1",
            params![book_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "Livro não encontrado".to_string())?;
    // NULL means the row is still on the pre-READ-32 layout, with the file
    // loose in `library/`. The boot migration moves it; until it does there is
    // no folder to write pages into.
    let folder = folder.ok_or_else(|| {
        "Livro ainda não tem pasta própria; reabra o app para migrar a biblioteca".to_string()
    })?;
    let dir = storage::book_dir(library_dir, &folder).map_err(|e| e.to_string())?;
    let file = dir.join(&filename);
    match extension_of(&file).as_str() {
        "pdf" | "epub" => Ok((file, dir)),
        // Reusing the existing error keeps one wording for "this app does not
        // read that": no new error type, no second message to translate.
        other => Err(ParseError::UnsupportedFormat(other.to_string()).to_string()),
    }
}

/// PDF goes through the extractor the document pipeline already uses, so the
/// hyphen repair measured against the user's Civil Code import (L-003) applies
/// to books too, with no second copy of it (READ-08).
fn extract(file: &Path) -> Result<String, String> {
    match extension_of(file).as_str() {
        "pdf" => crate::rag::parsing::extract_pdf(file).map_err(|e| e.to_string()),
        "epub" => epub::extract_epub_text(file).map_err(|e| e.to_string()),
        other => Err(ParseError::UnsupportedFormat(other.to_string()).to_string()),
    }
}

/// Deletes every language folder of the book, `original/` included.
///
/// Re-pagination moves every index, so a `pt/0047.txt` kept from the previous
/// run would point at the wrong passage — worse than being gone, because it
/// looks right (READ-13). The names go back through `storage::remove_lang` so
/// the single-component guard still has the last word on what gets deleted.
fn wipe_languages(book_dir: &Path) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(book_dir) {
        Ok(entries) => entries,
        // A book that was never processed has no folder yet.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            storage::remove_lang(book_dir, &entry.file_name().to_string_lossy())?;
        }
    }
    Ok(())
}

/// Extract → paginate → persist, for one book. Returns the page count.
///
/// The translation half of the design's flow is **not** here: `ready` happens
/// before any translation on purpose (the original is readable the moment
/// pagination ends), and T6 is what hangs the loop off the end of this
/// function.
pub(crate) fn process_into_pages(
    conn: &Connection,
    library_dir: &Path,
    book_id: &str,
    cancelled: &CancellationToken,
    emit: &mut dyn FnMut(BookStatusEvent),
) -> Result<u32, String> {
    let (file, dir) = book_paths(conn, library_dir, book_id)?;

    set_status(conn, book_id, BookStatus::Extracting, None, 0, emit)?;
    let text = match extract(&file) {
        Ok(text) => text,
        // Nothing has been deleted at this point: the wipe below only runs
        // once there is text to replace the pages with. A failed extraction
        // therefore leaves `original/` as it was — empty on a first run — and
        // `page_count` still true (READ-11).
        Err(message) => return Err(fail(conn, book_id, message, emit)),
    };

    if cancelled.is_cancelled() {
        // The only cancellation checkpoint this half of the flow has, and it
        // is deliberately coarse: extraction is one blocking call and
        // pagination is memory-fast. The per-paragraph checks live in the
        // translation loop (T6), which is where the hours are spent.
        set_status(conn, book_id, BookStatus::Imported, None, 0, emit)?;
        return Err("processamento cancelado".to_string());
    }

    set_status(conn, book_id, BookStatus::Paginating, None, 0, emit)?;
    let pages = pagination::paginate(&text);
    if pages.is_empty() {
        let message = ParseError::NoTextFound.to_string();
        return Err(fail(conn, book_id, message, emit));
    }

    if let Err(e) = wipe_languages(&dir) {
        return Err(fail(conn, book_id, e.to_string(), emit));
    }
    let original = match storage::lang_dir(&dir, storage::ORIGINAL_DIR) {
        Ok(path) => path,
        Err(e) => return Err(fail(conn, book_id, e.to_string(), emit)),
    };
    if let Err(e) = storage::write_pages(&original, &pages) {
        return Err(fail(conn, book_id, e.to_string(), emit));
    }

    let page_count = pages.len() as u32;
    // The files are on disk before `page_count` reaches the row, on purpose:
    // an interruption leaves extra files, never a `page_count` bigger than
    // reality — and the next run starts by clearing the folder.
    //
    // `MIN` is NULL-safe in SQLite, so a book never opened keeps its NULL
    // `last_page`, and a position saved against the old pagination is clamped
    // to the new last page instead of pointing past the end (READ-13).
    conn.execute(
        "UPDATE books SET page_count = ?1, last_page = MIN(last_page, ?2) WHERE id = ?3",
        params![page_count, page_count - 1, book_id],
    )
    .map_err(|e| fail(conn, book_id, e.to_string(), emit))?;

    set_status(conn, book_id, BookStatus::Ready, None, page_count, emit)?;
    Ok(page_count)
}

/// `extracting` and `paginating` are not resumable: there is no half
/// extraction worth keeping, and re-running it is cheap next to guessing what
/// survived a kill. Same reasoning as `requeue_unfinished_documents`.
pub(crate) fn reset_interrupted(conn: &Connection) -> usize {
    conn.execute(
        "UPDATE books SET status = 'imported' WHERE status IN ('extracting', 'paginating')",
        [],
    )
    .unwrap_or(0)
}

/// The boot half of `reset_interrupted`, called from `setup` next to
/// `migrate_legacy_layout`. Silent without a database: onboarding has not
/// finished, so there is no library yet.
pub fn reset_interrupted_processing(app: &AppHandle) {
    let db = app.state::<DbState>();
    let Ok(guard) = db.0.lock() else { return };
    let Some(conn) = guard.as_ref() else { return };
    let reset = reset_interrupted(conn);
    if reset > 0 {
        println!("reader: {reset} book(s) were interrupted mid-processing and went back to 'imported'");
    }
}

/// A book deleted mid-translation must not resurrect: the loop asks between
/// pages, the same guard `still_exists` gives the document pipeline. Without
/// it, the next page would recreate the folder that was just removed.
fn book_still_exists(app: &AppHandle, book_id: &str) -> bool {
    let db = app.state::<DbState>();
    let Ok(guard) = db.0.lock() else { return false };
    let Some(sql) = guard.as_ref() else {
        return false;
    };
    sql.query_row("SELECT 1 FROM books WHERE id = ?1", params![book_id], |_| {
        Ok(())
    })
    .optional()
    .map(|found| found.is_some())
    .unwrap_or(false)
}

/// The translation half of `process_book` **and** of `retranslate_pages`.
///
/// The loop itself is `translate::translate_book`; what is shared here is the
/// glue that only an `AppHandle` can provide — the client, the per-paragraph
/// future and the progress event. Two copies of it would drift, and the one
/// that drifted would translate one way on the new path and another way on the
/// old one.
///
/// Deleting page files is what the caller does *before* calling: the loop's
/// only question is "which page has no file yet".
async fn translate_language(
    app: &AppHandle,
    book_dir: &Path,
    book_id: &str,
    language: &str,
    model: &ActiveModel,
    page_count: u32,
    cancelled: &CancellationToken,
) -> Result<u32, String> {
    let client = crate::runtime_commands::client(app);
    translate::translate_book(
        book_dir,
        Some(language),
        page_count,
        cancelled,
        |paragraph| {
            let client = &client;
            async move { translate::translate_paragraph(client, model, language, &paragraph).await }
        },
        |done| {
            let _ = app.emit(
                "book-status",
                BookStatusEvent {
                    id: book_id.to_string(),
                    // The row stays `ready`: translating is work per language,
                    // and a status column could only ever describe one of them.
                    status: BookStatus::Ready,
                    language: Some(language.to_string()),
                    done,
                    total: page_count,
                    error_message: None,
                },
            );
            book_still_exists(app, book_id)
        },
    )
    .await
}

/// `language` is `None` for "do not translate", which is what the processing
/// dialog pre-selects. It is a parameter and not a column on purpose: a book
/// can be finished in `pt` and half done in `en` (READ-30).
///
/// Returns the page count. The translation half runs **after** `ready`, so an
/// interrupted or failed translation still leaves a readable book in
/// `original/`.
#[tauri::command]
pub async fn process_book(
    app: AppHandle,
    book_id: String,
    language: Option<String>,
) -> Result<u32, String> {
    let dir = crate::library_commands::library_dir(&app)?;

    // Resolving pdfium is `async` and the work below holds the database mutex,
    // so the path is read in a scope of its own: awaiting while holding a
    // `std::sync` guard is how a deadlock gets written by accident.
    let (file, book_dir) = {
        let db = app.state::<DbState>();
        let guard = db.0.lock().map_err(|e| e.to_string())?;
        book_paths(require_conn(&guard)?, &dir, &book_id)?
    };
    crate::rag::pdfium::ensure_for(&app, &file).await?;

    // The model is resolved before anything is extracted or deleted: a missing
    // model must not cost the user the pages he already had. `map` is also
    // what keeps "do not translate" from ever touching model selection.
    let model = match language {
        Some(_) => Some(translate::select_model(
            crate::runtime_commands::get_active_model(app.state::<DbState>())?,
        )?),
        None => None,
    };

    let cancelled = app.state::<CancellationRegistry>().register(&book_id);
    let page_count = {
        let db = app.state::<DbState>();
        let guard = db.0.lock().map_err(|e| e.to_string())?;
        process_into_pages(
            require_conn(&guard)?,
            &dir,
            &book_id,
            &cancelled,
            &mut |event| {
                let _ = app.emit("book-status", event);
            },
        )
    };
    // The database mutex is released before the translation loop: it runs for
    // ~63 min on a 300-page book (T1), and holding the connection for that
    // long would freeze every other screen in the app.
    let page_count = match page_count {
        Ok(count) => count,
        Err(e) => {
            app.state::<CancellationRegistry>().finish(&book_id);
            return Err(e);
        }
    };

    let translated = match (language.as_deref(), model.as_ref()) {
        (Some(language), Some(model)) => {
            translate_language(
                &app,
                &book_dir,
                &book_id,
                language,
                model,
                page_count,
                &cancelled,
            )
            .await
        }
        _ => Ok(0),
    };
    app.state::<CancellationRegistry>().finish(&book_id);

    // A failed translation is reported as a failed call, not as a failed book:
    // the row is `ready` and `original/` is readable. The pages that did come
    // out stay on disk and the next run resumes from the first missing one.
    translated?;
    Ok(page_count)
}

#[tauri::command]
pub fn cancel_processing(app: AppHandle, book_id: String) {
    app.state::<CancellationRegistry>().cancel(&book_id);
}


/// A page as the reader shows it, plus the language the text actually came
/// from — which is not always the one that was asked for.
///
/// Asking for page 47 in `pt` while `pt/0047.txt` has not been written yet is
/// the **normal** state of this feature, not an edge case: pages are
/// translated one by one while the book is already readable (READ-12). Both
/// obvious answers are wrong. Falling back to `original/` in silence would
/// hand the user English and let him believe it is the translation; refusing
/// would blank the reader in the middle of a translation that is working. So
/// the original is served **and named**, and the screen is what says so (T9).
#[derive(Debug, Clone, Serialize)]
pub struct BookPage {
    /// Zero-based, the same index `books.last_page` stores. The `{:04}.txt`
    /// files are base 1, and this is the only place the two meet.
    pub page: u32,
    pub page_count: u32,
    /// `original` or a language folder name — never `None`, because "which
    /// text is this" always has an answer.
    pub language: String,
    pub text: String,
}

/// One line of the sidebar's reading history (HIST-02).
#[derive(Debug, Clone, Serialize)]
pub struct ReadingEntry {
    pub id: String,
    pub filename: String,
    pub page_count: u32,
    /// Zero-based and already clamped, so the sidebar never renders a
    /// "page 41 of 10".
    pub last_page: u32,
    pub last_opened_at: String,
}

/// Positions are base 0: a 10-page book ends at 9, and a book with no pages
/// yet is at 0. Same convention `process_into_pages` clamps with (READ-13).
fn clamp_position(page: u32, page_count: u32) -> u32 {
    page.min(page_count.saturating_sub(1))
}

/// Records the opening and hands back where to resume (HIST-04, HIST-06).
///
/// The clamp is applied to what is returned and not written back: the row is
/// already clamped on every reprocess (READ-13), and opening a book must not
/// be a write that moves the user's saved position on its own.
pub(crate) fn open_position(conn: &Connection, book_id: &str) -> Result<u32, String> {
    let updated = conn
        .execute(
            "UPDATE books SET last_opened_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), book_id],
        )
        .map_err(|e| e.to_string())?;
    if updated == 0 {
        return Err("Livro não encontrado".to_string());
    }
    let (last_page, page_count): (Option<u32>, u32) = conn
        .query_row(
            "SELECT last_page, page_count FROM books WHERE id = ?1",
            params![book_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    // NULL is "never opened", which is page 0. The column stays NULL until a
    // position is actually saved, because what tells the history a book was
    // read is `last_opened_at`, not `last_page` (HIST-07).
    Ok(clamp_position(last_page.unwrap_or(0), page_count))
}

/// Persists the current page (HIST-05, READ-17).
///
/// The clamp is in the SQL because `page` arrives from the frontend: `MIN` and
/// `MAX` here cost one statement instead of a round trip for `page_count`, and
/// `MAX(page_count - 1, 0)` keeps a book with no pages yet at 0 instead of at
/// -1.
pub(crate) fn save_position(conn: &Connection, book_id: &str, page: u32) -> Result<(), String> {
    let updated = conn
        .execute(
            "UPDATE books SET last_page = MIN(?1, MAX(page_count - 1, 0)) WHERE id = ?2",
            params![page, book_id],
        )
        .map_err(|e| e.to_string())?;
    if updated == 0 {
        return Err("Livro não encontrado".to_string());
    }
    Ok(())
}

/// The text of one page, from the requested language when it exists.
///
/// `language` is a parameter and not the book's `reading_language` column: a
/// book can be ready in `pt` and half done in `en`, so the caller says which
/// one it wants and the column is only the stored preference (READ-30). `None`
/// and `original` both mean the extracted text.
pub(crate) fn page_text(
    conn: &Connection,
    library_dir: &Path,
    book_id: &str,
    page: u32,
    language: Option<&str>,
) -> Result<BookPage, String> {
    let (_, dir) = book_paths(conn, library_dir, book_id)?;
    let page_count: u32 = conn
        .query_row(
            "SELECT page_count FROM books WHERE id = ?1",
            params![book_id],
            |row| row.get(0),
        )
        .map_err(|_| "Livro não encontrado".to_string())?;
    if page_count == 0 {
        return Err("Livro ainda não foi processado".to_string());
    }
    if page >= page_count {
        return Err(format!(
            "página {} não existe: o livro tem {page_count}",
            page + 1
        ));
    }
    let number = page + 1;

    if let Some(language) = language.filter(|l| *l != storage::ORIGINAL_DIR) {
        let translated = storage::lang_dir(&dir, language).map_err(|e| e.to_string())?;
        if let Ok(text) = storage::read_page(&translated, number) {
            return Ok(BookPage {
                page,
                page_count,
                language: language.to_string(),
                text,
            });
        }
    }

    let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).map_err(|e| e.to_string())?;
    let text = storage::read_page(&original, number).map_err(|_| {
        // The book's folder can be deleted from the explorer while the row
        // stays: the library keeps listing it, and opening it has to say why
        // there is nothing to read instead of showing a blank page.
        "Os arquivos deste livro não estão mais no disco; reprocesse o livro".to_string()
    })?;
    Ok(BookPage {
        page,
        page_count,
        language: storage::ORIGINAL_DIR.to_string(),
        text,
    })
}

/// The books opened at least once, most recently opened first (HIST-02).
///
/// `WHERE last_opened_at IS NOT NULL` is what separates "imported" from
/// "read": the Library lists everything, the history lists only what the user
/// actually opened.
pub(crate) fn reading_history(conn: &Connection) -> Result<Vec<ReadingEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, filename, page_count, COALESCE(last_page, 0), last_opened_at
             FROM books WHERE last_opened_at IS NOT NULL ORDER BY last_opened_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let entries = stmt
        .query_map([], |row| {
            let page_count: u32 = row.get(2)?;
            let last_page: u32 = row.get(3)?;
            Ok(ReadingEntry {
                id: row.get(0)?,
                filename: row.get(1)?,
                page_count,
                last_page: clamp_position(last_page, page_count),
                last_opened_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(entries)
}

/// Returns the page to resume at, zero-based.
#[tauri::command]
pub fn open_book(db: State<DbState>, book_id: String) -> Result<u32, String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    open_position(require_conn(&guard)?, &book_id)
}

#[tauri::command]
pub fn save_reading_position(db: State<DbState>, book_id: String, page: u32) -> Result<(), String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    save_position(require_conn(&guard)?, &book_id, page)
}

#[tauri::command]
pub fn get_book_page(
    app: AppHandle,
    db: State<DbState>,
    book_id: String,
    page: u32,
    language: Option<String>,
) -> Result<BookPage, String> {
    let dir = crate::library_commands::library_dir(&app)?;
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    page_text(
        require_conn(&guard)?,
        &dir,
        &book_id,
        page,
        language.as_deref(),
    )
}

#[tauri::command]
pub fn list_reading_history(db: State<DbState>) -> Result<Vec<ReadingEntry>, String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    reading_history(require_conn(&guard)?)
}

/// One language folder of a book, with what is actually on disk in it.
#[derive(Debug, Clone, Serialize)]
pub struct BookLanguage {
    /// `original` or a language folder name.
    pub language: String,
    /// Counted from the files, never from the row: the folder is the user's
    /// and he can empty it from the explorer (READ-30, READ-31.9).
    pub pages: u32,
    /// Which one the reader is on, so the panel needs no sixth command to ask.
    pub reading: bool,
}

/// `original/` holds the extracted text, not a translation. Deleting it would
/// leave the row saying `ready` over a book with nothing to read, and
/// `language` arrives from the frontend — so every path that deletes asks
/// here first. Reading the original is still allowed; only erasing it is not.
fn translatable(language: &str) -> Result<(), String> {
    if language == storage::ORIGINAL_DIR {
        return Err(format!(
            "`{}` não é um idioma de tradução",
            storage::ORIGINAL_DIR
        ));
    }
    Ok(())
}

/// Marks pages to be made again by **deleting their files**, and hands back
/// what the loop needs: the book folder and its page count (READ-26).
///
/// There is no state column and no queue because there is nothing to add: the
/// resume checkpoint already is "the next page with no file", so an absent
/// file *is* the mark. The same reason it works identically when the one who
/// deleted was the user, in the explorer (READ-31.9).
///
/// `pages` is **base 0**, like `BookPage.page` and `save_reading_position` —
/// the `{:04}.txt` files are base 1 and the `+ 1` happens here, once. A page
/// past the end is refused instead of silently deleting nothing, which is the
/// only cheap way an off-by-one in the caller announces itself.
///
/// `None` means every page this language has. Nothing is re-extracted and
/// nothing is repaginated: re-extraction would move every index, and with it
/// the saved position and every other language's folders (READ-13 is the
/// command for that).
pub(crate) fn mark_for_retranslation(
    conn: &Connection,
    library_dir: &Path,
    book_id: &str,
    language: &str,
    pages: Option<&[u32]>,
) -> Result<(PathBuf, u32), String> {
    translatable(language)?;
    let (_, dir) = book_paths(conn, library_dir, book_id)?;
    let page_count: u32 = conn
        .query_row(
            "SELECT page_count FROM books WHERE id = ?1",
            params![book_id],
            |row| row.get(0),
        )
        .map_err(|_| "Livro não encontrado".to_string())?;
    if page_count == 0 {
        return Err("Livro ainda não foi processado".to_string());
    }
    let lang = storage::lang_dir(&dir, language).map_err(|e| e.to_string())?;

    let numbers: Vec<u32> = match pages {
        Some(pages) => {
            if let Some(beyond) = pages.iter().find(|page| **page >= page_count) {
                return Err(format!(
                    "página {} não existe: o livro tem {page_count}",
                    beyond + 1
                ));
            }
            pages.iter().map(|page| page + 1).collect()
        }
        None => storage::translated_pages(&lang).into_iter().collect(),
    };

    for number in numbers {
        match std::fs::remove_file(storage::page_file(&lang, number)) {
            Ok(()) => {}
            // Already absent is already marked: a page never translated, or
            // one the user deleted himself, needs no second deletion.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok((dir, page_count))
}

/// Deletes one language folder and says how many pages went with it (READ-29).
///
/// The count is taken **before** the deletion because afterwards there is
/// nothing left to count. Showing it to the user and asking for confirmation
/// is the panel's job (T15), which has `list_book_languages` for that.
pub(crate) fn remove_book_language(
    conn: &Connection,
    library_dir: &Path,
    book_id: &str,
    language: &str,
) -> Result<u32, String> {
    translatable(language)?;
    let (_, dir) = book_paths(conn, library_dir, book_id)?;
    let lang = storage::lang_dir(&dir, language).map_err(|e| e.to_string())?;
    let lost = storage::translated_pages(&lang).len() as u32;
    storage::remove_lang(&dir, language).map_err(|e| e.to_string())?;
    // Without this the reader would keep pointing at a folder that no longer
    // exists. NULL is the extracted text, which always exists.
    conn.execute(
        "UPDATE books SET reading_language = NULL WHERE id = ?1 AND reading_language = ?2",
        params![book_id, language],
    )
    .map_err(|e| e.to_string())?;
    Ok(lost)
}

/// Stores which folder the reader reads from (READ-27).
///
/// It deletes nothing. The previous design made this switch wipe every
/// translation because only one fitted; with one folder per language the
/// problem stopped existing instead of being mitigated — the user vetoed the
/// wipe on 2026-09-05.
pub(crate) fn set_book_reading_language(
    conn: &Connection,
    book_id: &str,
    language: Option<&str>,
) -> Result<(), String> {
    // `original` and NULL mean the same thing to `page_text`; storing NULL
    // keeps one representation of "the extracted text" instead of two.
    let stored = language.filter(|l| *l != storage::ORIGINAL_DIR);
    let updated = conn
        .execute(
            "UPDATE books SET reading_language = ?1 WHERE id = ?2",
            params![stored, book_id],
        )
        .map_err(|e| e.to_string())?;
    if updated == 0 {
        return Err("Livro não encontrado".to_string());
    }
    Ok(())
}

/// The language folders the book has, each with the pages present in it.
///
/// Every number comes from the disk (READ-30): a file deleted from the
/// explorer changes the count, which is exactly what a column could not do.
pub(crate) fn book_languages(
    conn: &Connection,
    library_dir: &Path,
    book_id: &str,
) -> Result<Vec<BookLanguage>, String> {
    let (_, dir) = book_paths(conn, library_dir, book_id)?;
    let reading: Option<String> = conn
        .query_row(
            "SELECT reading_language FROM books WHERE id = ?1",
            params![book_id],
            |row| row.get(0),
        )
        .map_err(|_| "Livro não encontrado".to_string())?;
    let reading = reading.unwrap_or_else(|| storage::ORIGINAL_DIR.to_string());

    // A book not processed yet — or whose folder the user deleted — has no
    // language at all, which is an empty list and not an error.
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut languages: Vec<BookLanguage> = entries
        .flatten()
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|entry| {
            let language = entry.file_name().to_string_lossy().into_owned();
            BookLanguage {
                pages: storage::translated_pages(&entry.path()).len() as u32,
                reading: language == reading,
                language,
            }
        })
        .collect();
    languages.sort_by(|a, b| a.language.cmp(&b.language));
    Ok(languages)
}

/// Redoes pages of one language: delete the files, then run the same loop
/// `process_book` runs.
///
/// `language` is a parameter and not the book's column: retranslating is
/// always *of a language*, and a book has several (READ-30). `pages` is base
/// 0, `None` meaning "redo this whole language".
#[tauri::command]
pub async fn retranslate_pages(
    app: AppHandle,
    book_id: String,
    language: String,
    pages: Option<Vec<u32>>,
) -> Result<(), String> {
    let library = crate::library_commands::library_dir(&app)?;
    // The model is resolved before a single file is deleted, the same order
    // `process_book` follows: a missing model must not cost the user the pages
    // he already had.
    let model = translate::select_model(crate::runtime_commands::get_active_model(
        app.state::<DbState>(),
    )?)?;

    let (book_dir, page_count) = {
        let db = app.state::<DbState>();
        let guard = db.0.lock().map_err(|e| e.to_string())?;
        mark_for_retranslation(
            require_conn(&guard)?,
            &library,
            &book_id,
            &language,
            pages.as_deref(),
        )?
    };

    // Same key `process_book` registers under, so `cancel_processing` cancels
    // this too — no second cancel command for the same book.
    let cancelled = app.state::<CancellationRegistry>().register(&book_id);
    let translated = translate_language(
        &app,
        &book_dir,
        &book_id,
        &language,
        &model,
        page_count,
        &cancelled,
    )
    .await;
    app.state::<CancellationRegistry>().finish(&book_id);
    translated.map(|_| ())
}

/// Starts a language the book does not have yet.
#[tauri::command]
pub async fn add_language(app: AppHandle, book_id: String, language: String) -> Result<(), String> {
    // Adding a language and redoing pages are the same run: the loop fills
    // every page with no file. The only difference is what was deleted first,
    // and adding deletes nothing — which is what the empty list says.
    retranslate_pages(app, book_id, language, Some(Vec::new())).await
}

/// Returns how many translated pages were deleted (READ-29).
#[tauri::command]
pub fn remove_language(
    app: AppHandle,
    db: State<DbState>,
    book_id: String,
    language: String,
) -> Result<u32, String> {
    let dir = crate::library_commands::library_dir(&app)?;
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    remove_book_language(require_conn(&guard)?, &dir, &book_id, &language)
}

/// `None` (or `original`) reads the extracted text. Deletes nothing (READ-27).
#[tauri::command]
pub fn set_reading_language(
    db: State<DbState>,
    book_id: String,
    language: Option<String>,
) -> Result<(), String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    set_book_reading_language(require_conn(&guard)?, &book_id, language.as_deref())
}

#[tauri::command]
pub fn list_book_languages(
    app: AppHandle,
    db: State<DbState>,
    book_id: String,
) -> Result<Vec<BookLanguage>, String> {
    let dir = crate::library_commands::library_dir(&app)?;
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    book_languages(require_conn(&guard)?, &dir, &book_id)
}
#[cfg(test)]
mod tests {
    //! ⚠️ Nenhum comando Tauri é exercitado aqui, e nenhum PDF real: não há
    //! runner de integração Tauri neste projeto, e `extract_pdf` precisa da
    //! biblioteca pdfium resolvida por `AppHandle`. O que estes testes provam é
    //! `process_into_pages` contra **banco em memória migrado + pasta
    //! temporária**, com EPUBs sintéticos montados aqui pelo crate `zip` — os
    //! mesmos fixtures da T3. Texto de livro real é a T13.

    use super::*;
    use std::io::Write;

    /// Uma biblioteca vazia por teste, sempre sob `std::env::temp_dir()` e
    /// **nunca** vinda da configuração do app: estas funções apagam pastas
    /// inteiras, e nenhuma delas pode ser a biblioteca real do usuário.
    fn library(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("readme-reader-cmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn migrated() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::apply_migrations(&mut conn).unwrap();
        conn
    }

    /// Um EPUB sintético com um capítulo de `paragraphs` parágrafos, cada um
    /// com `chars` caracteres — o tamanho é o que decide quantas páginas a
    /// paginação produz.
    fn write_epub(path: &Path, paragraphs: usize, chars: usize) {
        let body: String = (0..paragraphs)
            .map(|i| format!("<p>{}</p>", format!("p{i} ").repeat(chars / 4)))
            .collect();
        let opf = r#"<?xml version="1.0"?><package version="3.0" xmlns="http://www.idpf.org/2007/opf">
<manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="c1"/></spine></package>"#;
        let container = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            ("META-INF/container.xml", container.to_string()),
            ("OEBPS/content.opf", opf.to_string()),
            (
                "OEBPS/c1.xhtml",
                format!("<html><body>{body}</body></html>"),
            ),
        ] {
            writer.start_file::<_, ()>(name, opts).unwrap();
            writer.write_all(body.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    /// Uma linha de `books` com a pasta já criada, como `import_books` a
    /// deixa. Devolve a pasta do livro.
    fn insert_book(conn: &Connection, lib: &Path, id: &str, filename: &str) -> PathBuf {
        let folder = Path::new(filename).file_stem().unwrap().to_string_lossy().to_string();
        let dir = storage::book_dir(lib, &folder).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        conn.execute(
            "INSERT INTO books (id, filename, format, size_bytes, imported_at, folder)
             VALUES (?1, ?2, ?3, 1, '2026-09-06T00:00:00Z', ?4)",
            params![
                id,
                filename,
                extension_of(Path::new(filename)),
                folder
            ],
        )
        .unwrap();
        dir
    }

    fn process(conn: &Connection, lib: &Path, id: &str) -> Result<u32, String> {
        process_into_pages(conn, lib, id, &CancellationToken::default(), &mut |_| {})
    }

    /// Um livro de `pages` páginas já processado, com cada idioma de `langs`
    /// traduzido inteiro.
    ///
    /// Montado à mão em vez de por `process_into_pages`: as asserções da T14
    /// falam de "um livro de 10 páginas", e 10 tem de ser 10 — sair da
    /// paginação faria o número depender do fixture do EPUB.
    fn translated_book(
        conn: &Connection,
        lib: &Path,
        id: &str,
        pages: u32,
        langs: &[&str],
    ) -> PathBuf {
        let dir = insert_book(conn, lib, id, &format!("{id}.epub"));
        // O arquivo importado fica na pasta junto com as páginas, como
        // `import_books` o deixa: é ele que as asserções de "só a pasta do
        // idioma sumiu" precisam ver sobreviver.
        std::fs::write(dir.join(format!("{id}.epub")), b"bytes").unwrap();
        let bodies: Vec<String> = (1..=pages).map(|i| format!("Page {i}.")).collect();
        storage::write_pages(&storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap(), &bodies)
            .unwrap();
        for lang in langs {
            let bodies: Vec<String> = (1..=pages).map(|i| format!("[{lang}] Page {i}.")).collect();
            storage::write_pages(&storage::lang_dir(&dir, lang).unwrap(), &bodies).unwrap();
        }
        conn.execute(
            "UPDATE books SET page_count = ?1, status = 'ready' WHERE id = ?2",
            params![pages, id],
        )
        .unwrap();
        dir
    }

    /// Os bytes de cada página de uma pasta, para comparar antes e depois.
    fn bytes_in(dir: &Path) -> Vec<(u32, Vec<u8>)> {
        storage::translated_pages(dir)
            .into_iter()
            .map(|page| (page, std::fs::read(storage::page_file(dir, page)).unwrap()))
            .collect()
    }

    fn reading_language(conn: &Connection, id: &str) -> Option<String> {
        conn.query_row(
            "SELECT reading_language FROM books WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn status_of(conn: &Connection, id: &str) -> (String, Option<String>, i64, Option<i64>) {
        conn.query_row(
            "SELECT status, error_message, page_count, last_page FROM books WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap()
    }

    fn txt_files(dir: &Path) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .map(|e| {
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    txt_files(&e.path())
                } else {
                    usize::from(e.path().extension().is_some_and(|x| x == "txt"))
                }
            })
            .sum()
    }

    #[test]
    fn only_pdf_and_epub_can_be_processed() {
        // READ-02/READ-03. Os três formatos PalmDB são importáveis (LIB-01) e
        // não são legíveis: a recusa tem de vir com o nome do formato, e sem
        // mexer no estado do livro.
        let lib = library("formats");
        let conn = migrated();
        for (id, filename) in [("b1", "a.mobi"), ("b2", "a.azw"), ("b3", "a.azw3")] {
            insert_book(&conn, &lib, id, filename);
            let ext = extension_of(Path::new(filename));
            let err = process(&conn, &lib, id).unwrap_err();
            assert_eq!(err, format!("formato não suportado: .{ext}"), "{filename}");
            let (status, message, pages, _) = status_of(&conn, id);
            assert_eq!(status, "imported", "{filename} saiu de 'imported'");
            assert_eq!(message, None);
            assert_eq!(pages, 0);
        }
    }

    #[test]
    fn a_processed_book_fills_original_and_records_page_count() {
        let lib = library("pages");
        let conn = migrated();
        // 6 parágrafos de ~1.000 caracteres: acima de um único orçamento de
        // 2.500, para que a contagem de páginas não seja 1 por acidente.
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);

        let pages = process(&conn, &lib, "b1").unwrap();
        assert!(pages > 1, "o fixture coube numa página só: {pages}");

        let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap();
        assert_eq!(
            storage::translated_pages(&original).len() as u32,
            pages,
            "arquivos gravados e page_count retornado não batem"
        );
        let (status, message, recorded, last_page) = status_of(&conn, "b1");
        assert_eq!(status, "ready");
        assert_eq!(message, None);
        assert_eq!(recorded as u32, pages, "page_count no banco não bate");
        assert_eq!(last_page, None, "livro nunca aberto ganhou posição");
        assert!(storage::read_page(&original, 1).unwrap().contains("p0"));
    }

    #[test]
    fn extraction_failure_leaves_original_empty_and_page_count_zero() {
        // READ-11. O arquivo existe e não é um zip: a extração falha depois de
        // o status já ter ido para `extracting`, que é o caso perigoso.
        let lib = library("failure");
        let conn = migrated();
        let dir = insert_book(&conn, &lib, "b1", "quebrado.epub");
        std::fs::write(dir.join("quebrado.epub"), b"isto nao e um zip").unwrap();

        let err = process(&conn, &lib, "b1").unwrap_err();
        assert!(!err.is_empty());
        let (status, message, pages, _) = status_of(&conn, "b1");
        assert_eq!(status, "error");
        assert_eq!(message.as_deref(), Some(err.as_str()));
        assert_eq!(pages, 0);
        assert_eq!(txt_files(&dir), 0, "a falha gravou página");
    }

    #[test]
    fn reprocessing_wipes_every_language_folder_before_regenerating() {
        // READ-13. As traduções antigas estão presas a índices que a
        // repaginação muda; ficar com elas seria pior que apagá-las, porque
        // pareceriam certas.
        let lib = library("wipe");
        let conn = migrated();
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);
        process(&conn, &lib, "b1").unwrap();

        for lang in ["pt", "en"] {
            let lang_dir = storage::lang_dir(&dir, lang).unwrap();
            storage::write_pages(&lang_dir, &["página traduzida".to_string()]).unwrap();
            assert!(lang_dir.is_dir());
        }

        process(&conn, &lib, "b1").unwrap();

        for lang in ["pt", "en"] {
            assert!(
                !storage::lang_dir(&dir, lang).unwrap().exists(),
                "{lang}/ sobreviveu à repaginação"
            );
        }
        let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap();
        assert!(!storage::translated_pages(&original).is_empty());
    }

    #[test]
    fn removing_a_book_removes_its_whole_folder() {
        // READ-18/HIST-08, sobre o disco: não há tabela de páginas para uma FK
        // cascatear, então quem apaga é o código.
        let lib = library("remove");
        let conn = migrated();
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        let neighbour = insert_book(&conn, &lib, "b2", "outro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);
        write_epub(&neighbour.join("outro.epub"), 6, 1_000);
        process(&conn, &lib, "b1").unwrap();
        process(&conn, &lib, "b2").unwrap();
        storage::write_pages(
            &storage::lang_dir(&dir, "pt").unwrap(),
            &["traduzida".to_string()],
        )
        .unwrap();

        crate::library_commands::remove_book(&conn, &lib, "b1").unwrap();

        assert!(!dir.exists(), "a pasta do livro ficou no disco");
        assert!(neighbour.join("outro.epub").is_file(), "o vizinho foi junto");
        assert!(txt_files(&neighbour) > 0, "as páginas do vizinho foram junto");
    }

    #[test]
    fn reprocessing_into_fewer_pages_clamps_the_saved_position() {
        // READ-13: a posição salva é um índice, e a repaginação muda todos.
        // Apontar para o vazio é o defeito; clampar para a última página é a
        // escolha registrada — invalidar explicitamente, nunca deslocar em
        // silêncio.
        let lib = library("clamp-down");
        let conn = migrated();
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);
        process(&conn, &lib, "b1").unwrap();

        conn.execute("UPDATE books SET last_page = 40 WHERE id = 'b1'", [])
            .unwrap();
        // Um livro bem menor: o mesmo arquivo, com um capítulo de um parágrafo.
        write_epub(&dir.join("livro.epub"), 1, 100);
        let pages = process(&conn, &lib, "b1").unwrap();

        let (_, _, recorded, last_page) = status_of(&conn, "b1");
        assert_eq!(recorded as u32, pages);
        assert_eq!(
            last_page,
            Some(pages as i64 - 1),
            "a posição não foi clampada para a última página"
        );
        assert_ne!(last_page, Some(40));
    }

    #[test]
    fn reprocessing_into_more_pages_keeps_the_saved_position() {
        // O oposto do teste acima: com mais páginas do que antes, a posição
        // salva continua válida e não pode pular para o novo fim.
        let lib = library("clamp-up");
        let conn = migrated();
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 1, 100);
        process(&conn, &lib, "b1").unwrap();

        conn.execute("UPDATE books SET last_page = 0 WHERE id = 'b1'", [])
            .unwrap();
        write_epub(&dir.join("livro.epub"), 6, 1_000);
        let pages = process(&conn, &lib, "b1").unwrap();

        assert!(pages > 1);
        let (_, _, _, last_page) = status_of(&conn, "b1");
        assert_eq!(last_page, Some(0), "a posição salva se moveu sozinha");
    }

    #[test]
    fn processing_a_book_writes_nothing_to_documents() {
        // Herda a prova da LIB-08: ler um livro não é indexá-lo, e nenhuma
        // linha de `documents` (nem chunk, nem embedding) nasce daqui.
        let lib = library("no-rag");
        let conn = migrated();
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);

        process(&conn, &lib, "b1").unwrap();

        let documents: i64 = conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .unwrap();
        assert_eq!(documents, 0, "processar um livro criou linha em documents");
    }

    #[test]
    fn an_interrupted_processing_goes_back_to_imported_at_boot() {
        // Não há meia extração válida: um app fechado no meio deixaria o livro
        // preso em `extracting`, sem botão que o tirasse de lá.
        let conn = migrated();
        let lib = library("boot");
        for (id, status) in [
            ("b1", "extracting"),
            ("b2", "paginating"),
            ("b3", "ready"),
            ("b4", "error"),
        ] {
            insert_book(&conn, &lib, id, &format!("{id}.epub"));
            conn.execute(
                "UPDATE books SET status = ?1 WHERE id = ?2",
                params![status, id],
            )
            .unwrap();
        }

        assert_eq!(reset_interrupted(&conn), 2);
        assert_eq!(status_of(&conn, "b1").0, "imported");
        assert_eq!(status_of(&conn, "b2").0, "imported");
        assert_eq!(status_of(&conn, "b3").0, "ready", "um livro pronto foi mexido");
        assert_eq!(status_of(&conn, "b4").0, "error", "um livro com erro foi mexido");
    }

    #[test]
    fn the_status_sequence_is_announced_in_order() {
        // O frontend só sabe do progresso pelo evento: comandos Tauri são
        // request/response, e o retorno chega depois de tudo terminado.
        // ⚠️ Inconclusivo quanto ao `app.emit` real — o que roda aqui é a
        // closure; ninguém provou que o evento chega ao frontend (T13).
        let lib = library("events");
        let conn = migrated();
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);

        let mut seen = Vec::new();
        let pages = process_into_pages(
            &conn,
            &lib,
            "b1",
            &CancellationToken::default(),
            &mut |event| seen.push((event.status, event.total)),
        )
        .unwrap();

        assert_eq!(
            seen,
            vec![
                (BookStatus::Extracting, 0),
                (BookStatus::Paginating, 0),
                (BookStatus::Ready, pages),
            ]
        );
    }

    #[test]
    fn a_cancelled_processing_writes_no_page_and_goes_back_to_imported() {
        let lib = library("cancel");
        let conn = migrated();
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);

        let cancelled = CancellationToken::default();
        cancelled.cancel();
        let err = process_into_pages(&conn, &lib, "b1", &cancelled, &mut |_| {}).unwrap_err();

        assert_eq!(err, "processamento cancelado");
        assert_eq!(status_of(&conn, "b1").0, "imported");
        assert_eq!(txt_files(&dir), 0, "o cancelamento gravou página");
    }

    /// Grava `last_opened_at` direto, como a `book-library` já faz para
    /// `imported_at`: duas aberturas na mesma execução podem cair no mesmo
    /// instante e a ordenação ficaria indefinida, não DESC.
    fn opened_at(conn: &Connection, id: &str, when: &str) {
        conn.execute(
            "UPDATE books SET last_opened_at = ?1 WHERE id = ?2",
            params![when, id],
        )
        .unwrap();
    }

    fn last_opened(conn: &Connection, id: &str) -> Option<String> {
        conn.query_row(
            "SELECT last_opened_at FROM books WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn opening_a_book_records_the_moment_it_was_opened() {
        // HIST-04. O valor exato não é asserção: o teste só pode provar que a
        // coluna deixou de ser nula e que o que ficou lá é um RFC 3339
        // relegível. Que o instante seja "agora" na máquina do usuário é fé no
        // relógio do sistema, e nenhum teste desta suíte prova isso.
        let conn = migrated();
        let lib = library("open-records");
        insert_book(&conn, &lib, "b1", "livro.epub");
        assert_eq!(last_opened(&conn, "b1"), None, "nasceu já aberto");

        open_position(&conn, "b1").unwrap();

        let opened = last_opened(&conn, "b1").expect("abrir não gravou last_opened_at");
        chrono::DateTime::parse_from_rfc3339(&opened)
            .unwrap_or_else(|e| panic!("last_opened_at não é RFC 3339: {opened:?} ({e})"));
    }

    #[test]
    fn a_book_never_opened_starts_at_the_first_page() {
        // HIST-07. `last_page IS NULL` -> 0, e a coluna continua nula: quem
        // marca "foi lido" é `last_opened_at`, não uma posição inventada.
        let conn = migrated();
        let lib = library("open-first");
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);
        process(&conn, &lib, "b1").unwrap();
        assert_eq!(status_of(&conn, "b1").3, None, "posição nasceu preenchida");

        assert_eq!(open_position(&conn, "b1").unwrap(), 0);
        assert_eq!(status_of(&conn, "b1").3, None, "abrir gravou posição");
    }

    #[test]
    fn reopening_a_book_returns_the_saved_page() {
        // HIST-06/READ-16. Base 0 o tempo todo: salvar 4 devolve 4.
        let conn = migrated();
        let lib = library("reopen");
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 18, 1_000);
        let pages = process(&conn, &lib, "b1").unwrap();
        assert!(pages > 4, "o fixture não tem páginas suficientes: {pages}");

        save_position(&conn, "b1", 4).unwrap();
        assert_eq!(open_position(&conn, "b1").unwrap(), 4);
    }

    #[test]
    fn saving_a_position_persists_it() {
        // HIST-05/READ-17. A posição vai para a linha, e uma página além do
        // fim é clampada na gravação — o número vem do frontend.
        let conn = migrated();
        let lib = library("save-position");
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 18, 1_000);
        let pages = process(&conn, &lib, "b1").unwrap();

        save_position(&conn, "b1", 2).unwrap();
        assert_eq!(status_of(&conn, "b1").3, Some(2));

        save_position(&conn, "b1", 999).unwrap();
        assert_eq!(
            status_of(&conn, "b1").3,
            Some(pages as i64 - 1),
            "posição além do fim não foi clampada na gravação"
        );
    }

    #[test]
    fn the_history_lists_the_most_recently_opened_first() {
        // HIST-02.
        let conn = migrated();
        let lib = library("history-order");
        for (id, filename) in [("b1", "um.epub"), ("b2", "dois.epub"), ("b3", "tres.epub")] {
            insert_book(&conn, &lib, id, filename);
            open_position(&conn, id).unwrap();
        }
        opened_at(&conn, "b1", "2026-02-02T00:00:00+00:00");
        opened_at(&conn, "b2", "2026-03-03T00:00:00+00:00");
        opened_at(&conn, "b3", "2026-01-01T00:00:00+00:00");

        let ids: Vec<String> = reading_history(&conn)
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, vec!["b2", "b1", "b3"]);
    }

    #[test]
    fn an_imported_book_never_opened_is_not_in_the_history() {
        // `WHERE last_opened_at IS NOT NULL`: é o que separa "importado" de
        // "lido". A Biblioteca lista os dois; a lateral, só o segundo.
        let conn = migrated();
        let lib = library("history-imported");
        insert_book(&conn, &lib, "lido", "lido.epub");
        insert_book(&conn, &lib, "so-importado", "importado.epub");
        open_position(&conn, "lido").unwrap();

        let entries = reading_history(&conn).unwrap();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].id, "lido");
        assert_eq!(entries[0].filename, "lido.epub");
    }

    #[test]
    fn a_position_beyond_the_page_count_is_clamped_on_open() {
        // READ-16 contra livro reprocessado. A posição é escrita direto no
        // banco porque o caminho normal (`process_into_pages`) já clampa: o
        // que este teste cobre é a linha que ficou grande por qualquer outro
        // motivo, para o leitor nunca receber uma página inexistente.
        let conn = migrated();
        let lib = library("clamp-open");
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);
        let pages = process(&conn, &lib, "b1").unwrap();
        conn.execute(
            "UPDATE books SET last_page = 40 WHERE id = 'b1'",
            [],
        )
        .unwrap();

        assert_eq!(open_position(&conn, "b1").unwrap(), pages - 1);
    }

    #[test]
    fn a_page_without_a_translation_yet_comes_back_as_the_original() {
        // READ-30. Metade do livro traduzido é o estado normal desta feature,
        // não uma exceção: a página que já tem `pt/` volta em `pt`, e a que
        // ainda não tem volta em `original` **dizendo que é o original** — o
        // silêncio é que seria mentira. Rotular isso na tela é a T9, e nada
        // aqui prova que a tela o faz.
        let conn = migrated();
        let lib = library("page-language");
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);
        let pages = process(&conn, &lib, "b1").unwrap();
        assert!(pages > 1, "o fixture coube numa página só: {pages}");
        let pt = storage::lang_dir(&dir, "pt").unwrap();
        std::fs::create_dir_all(&pt).unwrap();
        std::fs::write(storage::page_file(&pt, 1), "primeira página em português").unwrap();

        let translated = page_text(&conn, &lib, "b1", 0, Some("pt")).unwrap();
        assert_eq!(translated.language, "pt");
        assert_eq!(translated.text, "primeira página em português");
        assert_eq!(translated.page_count, pages);

        let pending = page_text(&conn, &lib, "b1", 1, Some("pt")).unwrap();
        assert_eq!(pending.language, storage::ORIGINAL_DIR);
        assert_eq!(
            pending.text,
            storage::read_page(&storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap(), 2)
                .unwrap()
        );

        // Base 0 na fronteira, base 1 no disco: pedir a última página existe,
        // pedir a seguinte é erro em vez de arquivo não encontrado.
        assert!(page_text(&conn, &lib, "b1", pages - 1, None).is_ok());
        assert!(page_text(&conn, &lib, "b1", pages, None).is_err());
    }

    // ⚠️ Nenhum dos testes abaixo traduz coisa alguma: eles exercitam o que
    // APAGA e o que CONTA, que é a metade perigosa da T14. O laço que refaz a
    // página é `translate::translate_book` (T6, testado lá com um duble), e
    // `retranslate_pages`/`add_language` são comandos Tauri — não há runner de
    // integração neste projeto, então a cola entre os dois foi conferida por
    // leitura e NÃO por execução.

    #[test]
    fn retranslating_one_page_deletes_only_that_file() {
        // READ-26. `pages` é BASE 0, como `BookPage.page` e
        // `save_reading_position`: pedir a 3 apaga `0004.txt`. A conversão
        // acontece uma vez, dentro de `mark_for_retranslation`.
        let lib = library("retranslate-one");
        let conn = migrated();
        let dir = translated_book(&conn, &lib, "b1", 10, &["pt"]);
        let pt = storage::lang_dir(&dir, "pt").unwrap();

        let (folder, page_count) =
            mark_for_retranslation(&conn, &lib, "b1", "pt", Some(&[3])).unwrap();

        assert_eq!(folder, dir);
        assert_eq!(page_count, 10);
        assert!(!storage::page_file(&pt, 4).exists(), "0004.txt sobreviveu");
        assert_eq!(
            storage::translated_pages(&pt),
            (1..=10)
                .filter(|p| *p != 4)
                .collect::<std::collections::BTreeSet<u32>>(),
            "outra página foi apagada junto"
        );
        // O que o laço vai refazer é exatamente aquela, e nada antes dela.
        assert_eq!(storage::next_missing(&pt, 10), Some(4));
    }

    #[test]
    fn retranslating_pages_never_changes_page_count_or_the_original_files() {
        // READ-26: reprocessar página NÃO re-extrai. Re-extrair mudaria a
        // paginação e com ela todos os índices — a posição salva e as pastas
        // de todos os idiomas iriam junto.
        let lib = library("retranslate-original");
        let conn = migrated();
        let dir = translated_book(&conn, &lib, "b1", 10, &["pt"]);
        let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap();
        let before = bytes_in(&original);
        assert_eq!(before.len(), 10);

        mark_for_retranslation(&conn, &lib, "b1", "pt", Some(&[0, 4, 9])).unwrap();

        assert_eq!(bytes_in(&original), before, "original/ mudou ao remarcar");
        let (_, _, page_count, _) = status_of(&conn, "b1");
        assert_eq!(page_count, 10, "page_count mudou sem repaginação");
        // E `original/` não é apagável por este caminho: seria o livro inteiro.
        assert!(mark_for_retranslation(&conn, &lib, "b1", storage::ORIGINAL_DIR, None).is_err());
        assert_eq!(bytes_in(&original), before);
        // Uma página além do fim é recusada em vez de apagar nada em silêncio:
        // é assim que um erro de base no chamador aparece.
        assert!(mark_for_retranslation(&conn, &lib, "b1", "pt", Some(&[10])).is_err());
    }

    #[test]
    fn retranslating_one_language_leaves_the_other_untouched() {
        // READ-28. Uma pasta por idioma: apagar tudo de `pt/` não pode
        // encostar em `en/`.
        let lib = library("retranslate-other-lang");
        let conn = migrated();
        let dir = translated_book(&conn, &lib, "b1", 10, &["pt", "en"]);
        let en = storage::lang_dir(&dir, "en").unwrap();
        let before = bytes_in(&en);

        mark_for_retranslation(&conn, &lib, "b1", "pt", None).unwrap();

        let pt = storage::lang_dir(&dir, "pt").unwrap();
        assert!(
            storage::translated_pages(&pt).is_empty(),
            "sobrou página em pt"
        );
        assert_eq!(bytes_in(&en), before, "en/ foi junto");
        assert_eq!(storage::translated_pages(&en).len(), 10);
        assert_eq!(
            storage::translated_pages(&storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap())
                .len(),
            10
        );
    }

    #[test]
    fn retranslating_a_page_that_was_never_translated_is_a_successful_no_op() {
        // Arquivo ausente já é a marca: apagar de novo não é erro, e o idioma
        // que ninguém começou não vira pasta só por ter sido pedido.
        let lib = library("retranslate-noop");
        let conn = migrated();
        let dir = translated_book(&conn, &lib, "b1", 10, &[]);

        mark_for_retranslation(&conn, &lib, "b1", "pt", Some(&[3])).unwrap();
        assert!(
            !storage::lang_dir(&dir, "pt").unwrap().exists(),
            "remarcar criou a pasta do idioma"
        );

        // E de novo, agora com a pasta existindo e a página já ausente.
        storage::write_pages(
            &storage::lang_dir(&dir, "pt").unwrap(),
            &(1..=10).map(|i| format!("pt {i}")).collect::<Vec<_>>(),
        )
        .unwrap();
        mark_for_retranslation(&conn, &lib, "b1", "pt", Some(&[3])).unwrap();
        mark_for_retranslation(&conn, &lib, "b1", "pt", Some(&[3])).unwrap();
        assert_eq!(
            storage::translated_pages(&storage::lang_dir(&dir, "pt").unwrap()).len(),
            9
        );
    }

    #[test]
    fn changing_the_reading_language_deletes_nothing() {
        // READ-27, o inverso do que a versão anterior do plano fazia: trocar o
        // idioma de leitura limpava todas as traduções, porque só cabia uma.
        let lib = library("reading-language");
        let conn = migrated();
        let dir = translated_book(&conn, &lib, "b1", 10, &["pt", "en"]);
        let before: Vec<Vec<(u32, Vec<u8>)>> = [storage::ORIGINAL_DIR, "pt", "en"]
            .iter()
            .map(|lang| bytes_in(&storage::lang_dir(&dir, lang).unwrap()))
            .collect();
        assert_eq!(txt_files(&dir), 30);

        set_book_reading_language(&conn, "b1", Some("en")).unwrap();
        set_book_reading_language(&conn, "b1", Some("pt")).unwrap();

        let after: Vec<Vec<(u32, Vec<u8>)>> = [storage::ORIGINAL_DIR, "pt", "en"]
            .iter()
            .map(|lang| bytes_in(&storage::lang_dir(&dir, lang).unwrap()))
            .collect();
        assert_eq!(after, before, "trocar o idioma de leitura mexeu em arquivo");
        assert_eq!(txt_files(&dir), 30);
        assert_eq!(reading_language(&conn, "b1"), Some("pt".to_string()));
        // Voltar ao original é NULL, e não a string "original": `page_text`
        // trata os dois igual, e uma representação só é menos para lembrar.
        set_book_reading_language(&conn, "b1", Some(storage::ORIGINAL_DIR)).unwrap();
        assert_eq!(reading_language(&conn, "b1"), None);
        set_book_reading_language(&conn, "b1", None).unwrap();
        assert_eq!(reading_language(&conn, "b1"), None);
        assert_eq!(txt_files(&dir), 30);
    }

    #[test]
    fn removing_a_language_deletes_only_its_folder_and_reports_the_count_first() {
        // READ-29. A contagem vem do backend porque depois de apagar não há
        // mais o que contar. ⚠️ Que o painel MOSTRE essa contagem e peça
        // confirmação antes é T15, e nada aqui prova isso.
        let lib = library("remove-language");
        let conn = migrated();
        let dir = translated_book(&conn, &lib, "b1", 10, &["pt", "en"]);

        let lost = remove_book_language(&conn, &lib, "b1", "pt").unwrap();

        assert_eq!(lost, 10, "a contagem perdida não foi dita");
        assert!(!storage::lang_dir(&dir, "pt").unwrap().exists());
        assert_eq!(
            storage::translated_pages(&storage::lang_dir(&dir, "en").unwrap()).len(),
            10
        );
        assert_eq!(
            storage::translated_pages(&storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap())
                .len(),
            10
        );
        assert!(
            dir.join("b1.epub").is_file(),
            "o arquivo importado foi junto"
        );
        // Remover de novo é 0 e não erro: o usuário pode ter apagado a pasta.
        assert_eq!(remove_book_language(&conn, &lib, "b1", "pt").unwrap(), 0);
        // E `original/` não é removível: seria o livro, com a linha dizendo
        // `ready` sobre nada.
        assert!(remove_book_language(&conn, &lib, "b1", storage::ORIGINAL_DIR).is_err());
        assert!(storage::lang_dir(&dir, storage::ORIGINAL_DIR)
            .unwrap()
            .is_dir());
    }

    #[test]
    fn removing_the_language_being_read_falls_back_to_the_original() {
        // Sem isto o leitor continuaria apontando para uma pasta que não
        // existe mais.
        let lib = library("remove-read-language");
        let conn = migrated();
        let dir = translated_book(&conn, &lib, "b1", 10, &["pt", "en"]);
        set_book_reading_language(&conn, "b1", Some("pt")).unwrap();

        remove_book_language(&conn, &lib, "b1", "pt").unwrap();
        assert_eq!(reading_language(&conn, "b1"), None);

        // Remover um idioma que NÃO está sendo lido não mexe na coluna.
        set_book_reading_language(&conn, "b1", Some("en")).unwrap();
        storage::write_pages(
            &storage::lang_dir(&dir, "fr").unwrap(),
            &["une page".to_string()],
        )
        .unwrap();
        remove_book_language(&conn, &lib, "b1", "fr").unwrap();
        assert_eq!(reading_language(&conn, "b1"), Some("en".to_string()));
    }

    #[test]
    fn listing_languages_counts_pages_from_disk_not_from_the_database() {
        // READ-30. A pasta é do usuário: apagar um arquivo por fora tem de
        // mudar a contagem, que é justamente o que uma coluna não faria.
        let lib = library("list-languages");
        let conn = migrated();
        let dir = translated_book(&conn, &lib, "b1", 10, &["pt", "en"]);
        set_book_reading_language(&conn, "b1", Some("pt")).unwrap();

        let before = book_languages(&conn, &lib, "b1").unwrap();
        let names: Vec<&str> = before.iter().map(|l| l.language.as_str()).collect();
        assert_eq!(names, vec!["en", "original", "pt"]);
        assert!(before.iter().all(|l| l.pages == 10), "{before:?}");
        let reading: Vec<&str> = before
            .iter()
            .filter(|l| l.reading)
            .map(|l| l.language.as_str())
            .collect();
        assert_eq!(reading, vec!["pt"]);

        std::fs::remove_file(storage::page_file(
            &storage::lang_dir(&dir, "pt").unwrap(),
            4,
        ))
        .unwrap();

        let after = book_languages(&conn, &lib, "b1").unwrap();
        let pt = after.iter().find(|l| l.language == "pt").unwrap();
        assert_eq!(pt.pages, 9, "a contagem veio do banco, não do disco");
        assert_eq!(after.iter().find(|l| l.language == "en").unwrap().pages, 10);
        // E `page_count` continua 10: o livro não encolheu, uma tradução sim.
        assert_eq!(status_of(&conn, "b1").2, 10);
    }
}
