// SPEC: book-reader (READ-02, READ-03, READ-06, READ-08, READ-11, READ-12, READ-13, READ-14, READ-16,
//       READ-17, READ-20, READ-21, READ-26, READ-27, READ-28, READ-29, READ-30),
//       reading-history (HIST-02, HIST-04, HIST-05, HIST-06, HIST-07, HIST-09),
//       book-illustrations (ILLUS-03, ILLUS-07, ILLUS-09, ILLUS-10),
//       epub-fidelity (FID-01, FID-02, FID-03, FID-05, FID-09, FID-11, FID-12, FID-13),
//       book-library (LIB-13)

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
use crate::reader::illustrations::Illustration;
use crate::reader::{epub, html, pagination, storage, translate};
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
/// What one book turned into: the pages already cut, the pictures, the CSS,
/// and which format the pages are in.
struct Extracted {
    pages: Vec<String>,
    images: Vec<Illustration>,
    css: String,
    /// `html` for an EPUB read faithfully, `txt` for everything else.
    format: &'static str,
}

fn extract(file: &Path) -> Result<Extracted, String> {
    match extension_of(file).as_str() {
        "pdf" => {
            let (text, images) =
                crate::rag::parsing::extract_pdf_with_images(file).map_err(|e| e.to_string())?;
            Ok(Extracted {
                pages: pagination::paginate(&text),
                images,
                css: String::new(),
                format: "txt",
            })
        }
        // An EPUB is HTML+CSS already: keeping it is what makes the page look
        // like the book (FID-01). The text extractor stays as the fallback for
        // a file this cannot open structurally - a book that reads as plain
        // text beats a book that does not open.
        "epub" => match epub::extract_epub_html(file) {
            Ok(book) => Ok(Extracted {
                pages: html::paginate_chapters(&book.chapters),
                images: book.images,
                css: book.css,
                format: "html",
            }),
            Err(_) => {
                let (text, images) = epub::extract_epub(file).map_err(|e| e.to_string())?;
                Ok(Extracted {
                    pages: pagination::paginate(&text),
                    images,
                    css: String::new(),
                    format: "txt",
                })
            }
        },
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
        let name = entry.file_name().to_string_lossy().into_owned();
        // `images/` and `styles/` are siblings of the language folders, not
        // languages (FID-11). `write_images` and `write_css` are what clear
        // them, right after this, so deleting them here would only be a second
        // owner of the same folders.
        if entry.file_type()?.is_dir() && !storage::is_reserved_dir(&name) {
            storage::remove_lang(book_dir, &name)?;
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
///
/// `language` is the reading language the user picked in the dialog, `None`
/// for "do not translate". It is recorded here, next to the wipe, and not at
/// the end of the translation loop (READ-06, criterion 4) for two reasons
/// found in T13:
///
/// - the loop can be cancelled mid-book (READ-14) and the pages it already
///   wrote stay on disk. Writing at the end would leave a book with 40 of 300
///   pages in `pt` opening entirely in English — the opposite of what was
///   asked. Writing here costs nothing, because `get_book_page` already falls
///   back to `original/` **per page** and says so in its `language` field, so
///   an unfinished language never shows a blank reader;
/// - `None` has to reach the column too. `wipe_languages` below deletes
///   **every** language folder, so a reprocessing without translation must
///   not leave the row pointing at a folder this run just removed.
pub(crate) fn process_into_pages(
    conn: &Connection,
    library_dir: &Path,
    book_id: &str,
    language: Option<&str>,
    cancelled: &CancellationToken,
    emit: &mut dyn FnMut(BookStatusEvent),
) -> Result<u32, String> {
    let (file, dir) = book_paths(conn, library_dir, book_id)?;

    set_status(conn, book_id, BookStatus::Extracting, None, 0, emit)?;
    let extracted = match extract(&file) {
        Ok(extracted) => extracted,
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
    // The cut happened inside `extract`, because only it knows whether a page
    // is a run of characters or a list of blocks (FID-05).
    let Extracted {
        pages,
        images,
        css,
        format,
    } = extracted;
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
    if let Err(e) = storage::write_pages_ext(&original, &pages, format) {
        return Err(fail(conn, book_id, e.to_string(), emit));
    }
    if let Err(e) = storage::write_css(&dir, &css) {
        return Err(fail(conn, book_id, e.to_string(), emit));
    }
    // After the wipe and next to the pages, for the same reason: the markers
    // now on disk name these files, and a page pointing at a picture that was
    // never written is a broken image on screen (ILLUS-03, ILLUS-09).
    if let Err(e) = storage::write_images(&dir, &images) {
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
        "UPDATE books SET page_count = ?1 WHERE id = ?2",
        params![page_count, book_id],
    )
    .map_err(|e| fail(conn, book_id, e.to_string(), emit))?;
    // Every reading of the book, not one: since HIST-11 a book can be in the
    // history more than once, and each position has to stop pointing past the
    // new end.
    conn.execute(
        "UPDATE readings SET last_page = MIN(last_page, ?1) WHERE book_id = ?2",
        params![page_count - 1, book_id],
    )
    .map_err(|e| fail(conn, book_id, e.to_string(), emit))?;

    // Written before the `ready` event, so the row the frontend re-reads when
    // it sees `ready` already points at the folder the user asked for. Reuses
    // `set_book_reading_language` and not a second UPDATE: it is the one place
    // that maps `original` onto NULL, and two writers would be two chances to
    // disagree on how "the extracted text" is spelled.
    set_book_reading_language(conn, book_id, language)
        .map_err(|e| fail(conn, book_id, e, emit))?;

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
/// dialog pre-selects. What gets *translated* is a parameter and not a column
/// on purpose: a book can be finished in `pt` and half done in `en` (READ-30).
/// The choice is still recorded in `reading_language` — which folder to *read*
/// — by `process_into_pages` (READ-06); without it the user waited ~63 min for
/// a translation and then opened the book in English, with no error (T13).
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
            language.as_deref(),
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
    /// `html` when `text` is a whole document to drop into the reader's
    /// sandboxed iframe, `txt` when it is plain text (FID-02, FID-09).
    pub format: String,
}

/// One line of the sidebar's reading history (HIST-02).
#[derive(Debug, Clone, Serialize)]
pub struct ReadingEntry {
    /// The reading's id. Not the book's: one book can have several readings
    /// in the history (HIST-11).
    pub id: String,
    pub book_id: String,
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

/// A new reading of a book, at the first page, and its id (HIST-10).
///
/// Always a new row, never the existing one: "Ler" in the Library means
/// "start this book again", and the readings already in the history keep
/// their own positions (HIST-11).
pub(crate) fn start_reading_row(conn: &Connection, book_id: &str) -> Result<String, String> {
    // Checked here and not left to the foreign key: the error has to name the
    // book, and a connection without `foreign_keys = ON` would accept an orphan.
    conn.query_row("SELECT 1 FROM books WHERE id = ?1", params![book_id], |_| Ok(()))
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Livro não encontrado".to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO readings (id, book_id, last_page, last_opened_at) VALUES (?1, ?2, 0, ?3)",
        params![id, book_id, Utc::now().to_rfc3339()],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Records the opening of a reading and hands back where to resume (HIST-04,
/// HIST-06).
///
/// The clamp is applied to what is returned and not written back: the row is
/// already clamped on every reprocess (READ-13), and opening must not be a
/// write that moves the user's saved position on its own.
pub(crate) fn open_position(conn: &Connection, reading_id: &str) -> Result<u32, String> {
    let updated = conn
        .execute(
            "UPDATE readings SET last_opened_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), reading_id],
        )
        .map_err(|e| e.to_string())?;
    if updated == 0 {
        return Err("Leitura não encontrada".to_string());
    }
    let (last_page, page_count): (u32, u32) = conn
        .query_row(
            "SELECT r.last_page, b.page_count FROM readings r JOIN books b ON b.id = r.book_id
             WHERE r.id = ?1",
            params![reading_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    Ok(clamp_position(last_page, page_count))
}

/// Persists the current page (HIST-05, READ-17).
///
/// The clamp is in the SQL because `page` arrives from the frontend: `MIN` and
/// `MAX` here cost one statement instead of a round trip for `page_count`, and
/// `MAX(page_count - 1, 0)` keeps a book with no pages yet at 0 instead of at
/// -1.
pub(crate) fn save_position(conn: &Connection, reading_id: &str, page: u32) -> Result<(), String> {
    let updated = conn
        .execute(
            "UPDATE readings SET last_page = MIN(?1, MAX(
                (SELECT page_count FROM books WHERE books.id = readings.book_id) - 1, 0))
             WHERE id = ?2",
            params![page, reading_id],
        )
        .map_err(|e| e.to_string())?;
    if updated == 0 {
        return Err("Leitura não encontrada".to_string());
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
        if let Some((path, format)) = storage::existing_page(&translated, number) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                return Ok(rendered(page, page_count, language, &text, format, &dir));
            }
        }
    }

    let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).map_err(|e| e.to_string())?;
    let (path, format) = storage::existing_page(&original, number).ok_or_else(|| {
        // The book's folder can be deleted from the explorer while the row
        // stays: the library keeps listing it, and opening it has to say why
        // there is nothing to read instead of showing a blank page.
        "Os arquivos deste livro não estão mais no disco; reprocesse o livro".to_string()
    })?;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(rendered(
        page,
        page_count,
        storage::ORIGINAL_DIR,
        &text,
        format,
        &dir,
    ))
}

/// Turns a page file into what the reader shows.
///
/// For an HTML page that means **the whole document**: the book's stylesheet
/// and the page's blocks, with every `<img src="NNNN.ext">` swapped for a
/// `data:` URI (FID-02, FID-03). Assembling it here and not in the frontend is
/// what makes the iframe possible at all — a sandbox without
/// `allow-same-origin` has an opaque origin, so a `blob:` the app created is
/// unreadable inside it and no asset command would help.
///
/// ponytail: base64 inflates the bytes by about a third, per page, per view.
/// The upgrade is `allow-same-origin` plus `blob:` URLs, which loosens the
/// sandbox — measure a real illustrated book before paying that.
fn rendered(
    page: u32,
    page_count: u32,
    language: &str,
    text: &str,
    format: &'static str,
    book_dir: &Path,
) -> BookPage {
    let text = if format == "html" {
        document(&storage::read_css(book_dir), text, book_dir)
    } else {
        text.to_string()
    };
    BookPage {
        page,
        page_count,
        language: language.to_string(),
        text,
        format: format.to_string(),
    }
}

/// The base stylesheet, under the book's own. It only sets the page: the book
/// decides its typography, and anything here that the book also sets loses.
///
/// **No `max-width`.** A reading measure of ~38rem is the typographic answer and it
/// was what this had; the user asked for the page to use the whole panel, and the
/// panel is the window they chose. Resizing the window is how the measure is set now.
const READER_CSS: &str = "html{-webkit-text-size-adjust:100%}\
body{margin:0;padding:2rem 2.5rem;background:#fbfaf7;color:#1a1a1a;\
font-family:Georgia,'Times New Roman',serif;font-size:1.05rem;line-height:1.6;\
text-rendering:optimizeLegibility}\
img{max-width:100%;height:auto}\
p{margin:0 0 1em}";

fn document(css: &str, page_html: &str, book_dir: &Path) -> String {
    let body = inline_images(page_html, book_dir);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <style>{READER_CSS}</style><style>{css}</style></head><body>{body}</body></html>"
    )
}

/// Replaces `src="NNNN.ext"` with the file's bytes as a `data:` URI. A picture
/// that is not on disk keeps its name and simply does not render — one missing
/// image never costs the page.
fn inline_images(page_html: &str, book_dir: &Path) -> String {
    let mut out = String::with_capacity(page_html.len());
    let mut rest = page_html;
    while let Some(at) = rest.find("src=\"") {
        let after = &rest[at + 5..];
        let Some(end) = after.find('"') else { break };
        let name = &after[..end];
        out.push_str(&rest[..at + 5]);
        match storage::read_image(book_dir, name).ok() {
            Some(bytes) => {
                out.push_str(&format!("data:{};base64,{}", mime_of(name), base64(&bytes)));
            }
            None => out.push_str(name),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn mime_of(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, e)| e) {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        // JPEG is the default because it is what scanned plates in a book are,
        // and a wrong guess only costs the browser a sniff.
        _ => "image/jpeg",
    }
}

/// Standard base64. Sixteen lines instead of a dependency, and the alphabet is
/// fixed by RFC 4648 - there is nothing here to keep up to date.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Every reading, most recently opened first (HIST-02, HIST-11).
///
/// A book imported and never read has no row in `readings`, which is what
/// separates "imported" from "read": the Library lists everything, the history
/// lists only what the user actually opened. The `rowid` tiebreak keeps two
/// readings opened in the same instant in a stable order - newest row first.
pub(crate) fn reading_history(conn: &Connection) -> Result<Vec<ReadingEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.book_id, b.filename, b.page_count, r.last_page, r.last_opened_at
             FROM readings r JOIN books b ON b.id = r.book_id
             ORDER BY r.last_opened_at DESC, r.rowid DESC",
        )
        .map_err(|e| e.to_string())?;
    let entries = stmt
        .query_map([], |row| {
            let page_count: u32 = row.get(3)?;
            let last_page: u32 = row.get(4)?;
            Ok(ReadingEntry {
                id: row.get(0)?,
                book_id: row.get(1)?,
                filename: row.get(2)?,
                page_count,
                last_page: clamp_position(last_page, page_count),
                last_opened_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(entries)
}

/// Deletes one reading from the history (HIST-09).
///
/// Only that row: the book's other readings keep their positions, and nothing
/// on disk is touched - deleting the book itself is
/// `library_commands::remove_book` (HIST-08), a different action.
pub(crate) fn forget_position(conn: &Connection, reading_id: &str) -> Result<(), String> {
    let deleted = conn
        .execute("DELETE FROM readings WHERE id = ?1", params![reading_id])
        .map_err(|e| e.to_string())?;
    if deleted == 0 {
        return Err("Leitura não encontrada".to_string());
    }
    Ok(())
}

/// Starts a new reading of the book at the first page; returns its id (HIST-10).
#[tauri::command]
pub fn start_reading(db: State<DbState>, book_id: String) -> Result<String, String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    start_reading_row(require_conn(&guard)?, &book_id)
}

/// Returns the page to resume the reading at, zero-based.
#[tauri::command]
pub fn open_reading(db: State<DbState>, reading_id: String) -> Result<u32, String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    open_position(require_conn(&guard)?, &reading_id)
}

#[tauri::command]
pub fn save_reading_position(
    db: State<DbState>,
    reading_id: String,
    page: u32,
) -> Result<(), String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    save_position(require_conn(&guard)?, &reading_id, page)
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

/// The bytes of one illustration (ILLUS-07).
///
/// Raw bytes through `tauri::ipc::Response`, not `convertFileSrc`: the asset
/// protocol is disabled in `tauri.conf.json` and there is no asset permission
/// in the generated schema, so that route would mean new config, new
/// capabilities and a runtime scope over a folder that lives outside the
/// repository. This command needs none of the three.
///
/// `name` is the only string of this feature that comes back from the
/// frontend. `storage::read_image` is what refuses anything that is not
/// exactly `NNNN.<ext>`, so the check has one home and not two.
#[tauri::command]
pub fn get_book_image(
    app: AppHandle,
    db: State<DbState>,
    book_id: String,
    name: String,
) -> Result<tauri::ipc::Response, String> {
    let library = crate::library_commands::library_dir(&app)?;
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    let (_, dir) = book_paths(require_conn(&guard)?, &library, &book_id)?;
    let bytes = storage::read_image(&dir, &name).map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// The cover of a book as raw bytes, **empty** when it declares none (LIB-13).
///
/// Empty and not an error: a book without a cover is ordinary, and the row
/// shows a placeholder for it. Same transport as `get_book_image`, for the
/// same reasons.
///
/// ponytail: the EPUB is opened once per row every time the Library mounts,
/// with no copy on disk. Cache the bytes next to `images/` if a large library
/// measures slow.
#[tauri::command]
pub fn get_book_cover(
    app: AppHandle,
    db: State<DbState>,
    book_id: String,
) -> Result<tauri::ipc::Response, String> {
    let library = crate::library_commands::library_dir(&app)?;
    let (file, _) = {
        let guard = db.0.lock().map_err(|e| e.to_string())?;
        book_paths(require_conn(&guard)?, &library, &book_id)?
    };
    // The lock is released before the zip is read: a list of covers must not
    // hold the database while it opens every book.
    let bytes = epub::cover_image(&file).unwrap_or_default();
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn list_reading_history(db: State<DbState>) -> Result<Vec<ReadingEntry>, String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    reading_history(require_conn(&guard)?)
}

/// Deletes one reading. The book, its file and every translation folder stay
/// on disk (HIST-09).
#[tauri::command]
pub fn forget_reading_entry(db: State<DbState>, reading_id: String) -> Result<(), String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    forget_position(require_conn(&guard)?, &reading_id)
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
        // Without this the illustrations and stylesheet folders would show up
        // in the reader's language picker as languages called "images" and
        // "styles", with absurd page counts (FID-11).
        .filter(|entry| !storage::is_reserved_dir(&entry.file_name().to_string_lossy()))
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

    /// O mesmo EPUB de `write_epub`, com uma gravura no fim do capítulo.
    /// Os bytes não precisam ser um PNG de verdade: nada nesta metade do
    /// caminho decodifica a imagem — o EPUB traz o arquivo pronto.
    fn write_illustrated_epub(path: &Path, images: usize) {
        let body: String = (0..images)
            .map(|i| format!("<p>parágrafo {i}</p><img src=\"img{i}.png\"/>"))
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
        writer.start_file::<_, ()>("META-INF/container.xml", opts).unwrap();
        writer.write_all(container.as_bytes()).unwrap();
        writer.start_file::<_, ()>("OEBPS/content.opf", opts).unwrap();
        writer.write_all(opf.as_bytes()).unwrap();
        writer.start_file::<_, ()>("OEBPS/c1.xhtml", opts).unwrap();
        writer
            .write_all(format!("<html><body>{body}</body></html>").as_bytes())
            .unwrap();
        for i in 0..images {
            writer
                .start_file::<_, ()>(format!("OEBPS/img{i}.png"), opts)
                .unwrap();
            writer.write_all(format!("bytes-{i}").as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn the_images_folder_is_not_a_language_and_survives_the_wipe_that_eats_languages() {
        // ILLUS-10. Sem as duas exceções, `images/` apareceria no seletor de
        // idioma do leitor com uma contagem de páginas absurda, e sumiria no
        // primeiro reprocessamento.
        let conn = migrated();
        let lib = library("images-not-a-language");
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_illustrated_epub(&dir.join("livro.epub"), 3);

        process(&conn, &lib, "b1").unwrap();

        let images = storage::images_dir(&dir).unwrap();
        assert!(images.exists(), "as gravuras não foram gravadas");
        assert_eq!(storage::read_image(&dir, "0002.png").unwrap(), b"bytes-1");

        let languages = book_languages(&conn, &lib, "b1").unwrap();
        assert_eq!(
            languages.iter().map(|l| l.language.as_str()).collect::<Vec<_>>(),
            vec!["original"],
            "`images/` foi listada como idioma"
        );

        // Reprocessar: o wipe apaga os idiomas e NÃO pode levar as gravuras;
        // quem as substitui é `write_images` (ILLUS-09).
        write_illustrated_epub(&dir.join("livro.epub"), 1);
        process(&conn, &lib, "b1").unwrap();

        let mut names: Vec<String> = std::fs::read_dir(&images)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["0001.png"], "sobrou gravura da rodada anterior");
        assert_eq!(storage::read_image(&dir, "0001.png").unwrap(), b"bytes-0");
    }

    #[test]
    fn every_image_written_is_pointed_at_by_a_page_and_arrives_inlined() {
        // FID-03, nos dois pontos onde as metades se encontram: o `<img>` na
        // página em disco aponta para um arquivo que existe, e o documento
        // que chega à tela traz os bytes dentro dele — um iframe sandbox tem
        // origem opaca e não buscaria o arquivo sozinho.
        let conn = migrated();
        let lib = library("images-match-pages");
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_illustrated_epub(&dir.join("livro.epub"), 4);

        let page_count = process(&conn, &lib, "b1").unwrap();

        let original = storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap();
        assert_eq!(storage::existing_page(&original, 1).unwrap().1, "html");
        let all: String = (1..=page_count)
            .map(|p| storage::read_page(&original, p).unwrap())
            .collect::<Vec<_>>()
            .join("\n\n");
        for i in 1..=4 {
            let name = format!("{i:04}.png");
            assert!(all.contains(&format!("src=\"{name}\"")), "página nenhuma cita {name}");
            assert!(storage::read_image(&dir, &name).is_ok(), "{name} não está em disco");
        }
        assert!(storage::read_image(&dir, "0005.png").is_err());
        // O nome não sobrevive na tela: ele vira os bytes (ponytail: base64).
        let shown = page_text(&conn, &lib, "b1", 0, None).unwrap();
        assert!(!shown.text.contains("src=\"0001.png\""));
        assert!(shown.text.contains("src=\"data:image/png;base64,"));
    }

    /// `language: None` = "não traduzir", o padrão do diálogo. Os testes que
    /// olham a coluna passam o idioma direto por `process_into_pages`.
    fn process(conn: &Connection, lib: &Path, id: &str) -> Result<u32, String> {
        process_into_pages(conn, lib, id, None, &CancellationToken::default(), &mut |_| {})
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

    /// Page files of either format, recursively (FID-09).
    fn page_files(dir: &Path) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .map(|e| {
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    page_files(&e.path())
                } else {
                    usize::from(
                        e.path()
                            .extension()
                            .is_some_and(|x| x == "txt" || x == "html"),
                    )
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
        assert_eq!(page_files(&dir), 0, "a falha gravou página");
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
        assert!(page_files(&neighbour) > 0, "as páginas do vizinho foram junto");
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

        // Duas leituras do mesmo livro (HIST-11): as duas têm de ser clampadas.
        let r1 = start_reading_row(&conn, "b1").unwrap();
        let r2 = start_reading_row(&conn, "b1").unwrap();
        conn.execute("UPDATE readings SET last_page = 40", []).unwrap();
        // Um livro bem menor: o mesmo arquivo, com um capítulo de um parágrafo.
        write_epub(&dir.join("livro.epub"), 1, 100);
        let pages = process(&conn, &lib, "b1").unwrap();

        let (_, _, recorded, _) = status_of(&conn, "b1");
        assert_eq!(recorded as u32, pages);
        for reading in [&r1, &r2] {
            assert_eq!(
                reading_page(&conn, reading),
                pages as i64 - 1,
                "uma leitura não foi clampada para a última página"
            );
        }
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

        let reading = start_reading_row(&conn, "b1").unwrap();
        write_epub(&dir.join("livro.epub"), 6, 1_000);
        let pages = process(&conn, &lib, "b1").unwrap();

        assert!(pages > 1);
        assert_eq!(reading_page(&conn, &reading), 0, "a posição salva se moveu sozinha");
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
            None,
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
        let err =
            process_into_pages(&conn, &lib, "b1", None, &cancelled, &mut |_| {}).unwrap_err();

        assert_eq!(err, "processamento cancelado");
        assert_eq!(status_of(&conn, "b1").0, "imported");
        assert_eq!(page_files(&dir), 0, "o cancelamento gravou página");
    }

    /// Grava `last_opened_at` direto, como a `book-library` já faz para
    /// `imported_at`: duas aberturas na mesma execução podem cair no mesmo
    /// instante e a ordenação ficaria indefinida, não DESC.
    fn opened_at(conn: &Connection, reading: &str, when: &str) {
        conn.execute(
            "UPDATE readings SET last_opened_at = ?1 WHERE id = ?2",
            params![when, reading],
        )
        .unwrap();
    }

    fn last_opened(conn: &Connection, reading: &str) -> Option<String> {
        conn.query_row(
            "SELECT last_opened_at FROM readings WHERE id = ?1",
            params![reading],
            |row| row.get(0),
        )
        .optional()
        .unwrap()
    }

    fn reading_page(conn: &Connection, reading: &str) -> i64 {
        conn.query_row(
            "SELECT last_page FROM readings WHERE id = ?1",
            params![reading],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn opening_a_book_records_the_moment_it_was_opened() {
        // HIST-04. O valor exato não é asserção: o teste só pode provar que a
        // coluna foi gravada e que o que ficou lá é um RFC 3339 relegível. Que
        // o instante seja "agora" na máquina do usuário é fé no relógio do
        // sistema, e nenhum teste desta suíte prova isso.
        let conn = migrated();
        let lib = library("open-records");
        insert_book(&conn, &lib, "b1", "livro.epub");
        let reading = start_reading_row(&conn, "b1").unwrap();
        opened_at(&conn, &reading, "antes");

        open_position(&conn, &reading).unwrap();

        let opened = last_opened(&conn, &reading).expect("a leitura sumiu");
        chrono::DateTime::parse_from_rfc3339(&opened)
            .unwrap_or_else(|e| panic!("last_opened_at não é RFC 3339: {opened:?} ({e})"));
    }

    #[test]
    fn a_book_never_opened_starts_at_the_first_page() {
        // HIST-07/HIST-10. Uma leitura nova começa na primeira página, e só
        // existe depois do "Ler": importar não cria leitura nenhuma.
        let conn = migrated();
        let lib = library("open-first");
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);
        process(&conn, &lib, "b1").unwrap();
        assert!(reading_history(&conn).unwrap().is_empty(), "processar criou leitura");

        let reading = start_reading_row(&conn, "b1").unwrap();

        assert_eq!(open_position(&conn, &reading).unwrap(), 0);
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
        let reading = start_reading_row(&conn, "b1").unwrap();

        save_position(&conn, &reading, 4).unwrap();
        assert_eq!(open_position(&conn, &reading).unwrap(), 4);
    }

    #[test]
    fn saving_a_position_persists_it() {
        // HIST-05/READ-17. A posição vai para a leitura, e uma página além do
        // fim é clampada na gravação — o número vem do frontend.
        let conn = migrated();
        let lib = library("save-position");
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 18, 1_000);
        let pages = process(&conn, &lib, "b1").unwrap();
        let reading = start_reading_row(&conn, "b1").unwrap();

        save_position(&conn, &reading, 2).unwrap();
        assert_eq!(reading_page(&conn, &reading), 2);

        save_position(&conn, &reading, 999).unwrap();
        assert_eq!(
            reading_page(&conn, &reading),
            pages as i64 - 1,
            "posição além do fim não foi clampada na gravação"
        );
    }

    #[test]
    fn the_history_lists_the_most_recently_opened_first() {
        // HIST-02.
        let conn = migrated();
        let lib = library("history-order");
        let mut readings = Vec::new();
        for (id, filename) in [("b1", "um.epub"), ("b2", "dois.epub"), ("b3", "tres.epub")] {
            insert_book(&conn, &lib, id, filename);
            readings.push(start_reading_row(&conn, id).unwrap());
        }
        opened_at(&conn, &readings[0], "2026-02-02T00:00:00+00:00");
        opened_at(&conn, &readings[1], "2026-03-03T00:00:00+00:00");
        opened_at(&conn, &readings[2], "2026-01-01T00:00:00+00:00");

        let books: Vec<String> = reading_history(&conn)
            .unwrap()
            .into_iter()
            .map(|e| e.book_id)
            .collect();
        assert_eq!(books, vec!["b2", "b1", "b3"]);
    }

    #[test]
    fn reading_a_book_again_is_a_new_entry_at_the_first_page() {
        // HIST-10/HIST-11, o pedido: "Ler" de novo abre outra leitura na
        // página 1, e a anterior continua no histórico com a página dela.
        let conn = migrated();
        let lib = library("read-again");
        translated_book(&conn, &lib, "b1", 10, &[]);
        let first = start_reading_row(&conn, "b1").unwrap();
        save_position(&conn, &first, 6).unwrap();

        let second = start_reading_row(&conn, "b1").unwrap();

        assert_ne!(first, second, "reaproveitou a leitura existente");
        assert_eq!(open_position(&conn, &second).unwrap(), 0);
        assert_eq!(open_position(&conn, &first).unwrap(), 6, "a leitura antiga perdeu a página");
        let entries = reading_history(&conn).unwrap();
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert!(entries.iter().all(|e| e.book_id == "b1"));
    }

    #[test]
    fn starting_a_reading_of_a_book_that_does_not_exist_is_an_error() {
        let conn = migrated();
        assert!(start_reading_row(&conn, "nao-existe").is_err());
        assert!(reading_history(&conn).unwrap().is_empty(), "gravou leitura órfã");
    }

    #[test]
    fn deleting_a_history_entry_forgets_the_position_and_keeps_the_book() {
        // HIST-09, AC 2/3/5. O oposto de `removing_a_book_removes_its_whole_folder`:
        // aqui nada em disco é tocado — só a linha da leitura sai.
        let lib = library("forget-entry");
        let conn = migrated();
        let dir = translated_book(&conn, &lib, "b1", 10, &["pt", "en"]);
        let neighbour = translated_book(&conn, &lib, "b2", 4, &[]);
        let files_before = page_files(&dir);
        let r1 = start_reading_row(&conn, "b1").unwrap();
        let r2 = start_reading_row(&conn, "b2").unwrap();
        save_position(&conn, &r1, 7).unwrap();
        save_position(&conn, &r2, 2).unwrap();

        forget_position(&conn, &r1).unwrap();

        // AC 2: a leitura some inteira, posição junto.
        assert_eq!(last_opened(&conn, &r1), None, "a leitura sobreviveu");
        // AC 5: a outra entrada continua no histórico, na posição dela.
        let entries = reading_history(&conn).unwrap();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].id, r2);
        assert_eq!(entries[0].last_page, 2, "a posição do vizinho foi junto");
        // AC 3: o livro continua na Biblioteca, com o arquivo importado e as
        // duas pastas de tradução intactas.
        assert!(dir.join("b1.epub").is_file(), "o arquivo importado sumiu");
        for lang in [storage::ORIGINAL_DIR, "pt", "en"] {
            assert!(
                storage::lang_dir(&dir, lang).unwrap().is_dir(),
                "{lang}/ sumiu do disco"
            );
        }
        assert_eq!(page_files(&dir), files_before, "páginas foram apagadas");
        assert!(neighbour.join("b2.epub").is_file());
        let (status, _, page_count, _) = status_of(&conn, "b1");
        assert_eq!((status.as_str(), page_count), ("ready", 10));
    }

    #[test]
    fn a_book_deleted_from_the_history_reopens_at_the_first_page() {
        // HIST-09, AC 4. A leitura apagada não reabre; ler o livro de novo é
        // uma leitura nova, na primeira página, sem ressuscitar a antiga.
        let lib = library("forget-reopen");
        let conn = migrated();
        translated_book(&conn, &lib, "b1", 10, &[]);
        let old = start_reading_row(&conn, "b1").unwrap();
        save_position(&conn, &old, 7).unwrap();
        assert_eq!(open_position(&conn, &old).unwrap(), 7);

        forget_position(&conn, &old).unwrap();

        assert!(open_position(&conn, &old).is_err(), "a leitura apagada reabriu");
        let again = start_reading_row(&conn, "b1").unwrap();
        assert_eq!(open_position(&conn, &again).unwrap(), 0);
        // Ler de novo devolve o livro ao histórico — apagar esquece a leitura,
        // não proíbe o livro.
        assert_eq!(reading_history(&conn).unwrap().len(), 1);
    }

    #[test]
    fn forgetting_a_book_that_does_not_exist_is_an_error() {
        // Mesmo contrato de `open_position`/`save_position`: 0 linhas afetadas
        // vira erro em vez de sucesso silencioso.
        let conn = migrated();
        assert!(forget_position(&conn, "nao-existe").is_err());
        assert!(open_position(&conn, "nao-existe").is_err());
        assert!(save_position(&conn, "nao-existe", 1).is_err());
    }

    #[test]
    fn an_imported_book_never_opened_is_not_in_the_history() {
        // Sem linha em `readings`, sem histórico: é o que separa "importado"
        // de "lido". A Biblioteca lista os dois; a lateral, só o segundo.
        let conn = migrated();
        let lib = library("history-imported");
        insert_book(&conn, &lib, "lido", "lido.epub");
        insert_book(&conn, &lib, "so-importado", "importado.epub");
        start_reading_row(&conn, "lido").unwrap();

        let entries = reading_history(&conn).unwrap();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].book_id, "lido");
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
        let reading = start_reading_row(&conn, "b1").unwrap();
        conn.execute(
            "UPDATE readings SET last_page = 40 WHERE id = ?1",
            params![reading],
        )
        .unwrap();

        assert_eq!(open_position(&conn, &reading).unwrap(), pages - 1);
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
        // O EPUB agora sai em HTML, e `page_text` devolve o documento montado
        // para o iframe (FID-02) — a página do livro está dentro dele.
        assert_eq!(pending.format, "html");
        let on_disk =
            storage::read_page(&storage::lang_dir(&dir, storage::ORIGINAL_DIR).unwrap(), 2)
                .unwrap();
        assert!(
            pending.text.contains(&on_disk),
            "o documento montado não contém a página que está em disco"
        );
        assert!(pending.text.starts_with("<!doctype html>"));
        // A página escrita à mão como `.txt` continua saindo como texto puro,
        // que é o caminho do formato antigo (FID-09).
        assert_eq!(translated.format, "txt");

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
    fn processing_records_the_chosen_language_on_the_book() {
        // READ-06, critério 4. O defeito que este teste tranca (T13, defeito
        // 3): `process_book` recebia o idioma, traduzia para ele e NUNCA
        // escrevia a coluna. O usuário esperava ~63 min por uma tradução em
        // `pt` e o leitor abria em inglês, sem erro nenhum — nenhum gate
        // pegava, porque escrita ausente não é tipo errado.
        //
        // ⚠️ O que roda aqui é `process_into_pages` contra banco em memória +
        // pasta temporária. A tradução em si NÃO é exercitada (precisa do
        // sidecar), e ninguém viu o livro abrir traduzido na tela: isso
        // continua sendo o UAT da T13.
        let lib = library("records-language");
        let conn = migrated();
        let dir = insert_book(&conn, &lib, "b1", "livro.epub");
        write_epub(&dir.join("livro.epub"), 6, 1_000);

        // Gravado ao fim da paginação, não ao fim da tradução: uma tradução
        // cancelada no meio (READ-14) deixa páginas em disco, e o livro tem de
        // abrir no idioma pedido com as que faltam caindo para `original/`.
        process_into_pages(
            &conn,
            &lib,
            "b1",
            Some("pt"),
            &CancellationToken::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(
            reading_language(&conn, "b1"),
            Some("pt".to_string()),
            "o idioma escolhido não foi registrado no livro"
        );

        // Reprocessar sem traduzir tem de LIMPAR a coluna: `wipe_languages`
        // acabou de apagar a pasta `pt/`, e a linha não pode continuar
        // apontando para ela.
        process(&conn, &lib, "b1").unwrap();
        assert_eq!(
            reading_language(&conn, "b1"),
            None,
            "o livro continuou apontando para um idioma que foi apagado"
        );

        // Uma representação só para "o texto extraído", a mesma decisão que
        // `set_book_reading_language` já toma: `original` é gravado como NULL.
        process_into_pages(
            &conn,
            &lib,
            "b1",
            Some(storage::ORIGINAL_DIR),
            &CancellationToken::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(reading_language(&conn, "b1"), None);
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
        assert_eq!(page_files(&dir), 30);

        set_book_reading_language(&conn, "b1", Some("en")).unwrap();
        set_book_reading_language(&conn, "b1", Some("pt")).unwrap();

        let after: Vec<Vec<(u32, Vec<u8>)>> = [storage::ORIGINAL_DIR, "pt", "en"]
            .iter()
            .map(|lang| bytes_in(&storage::lang_dir(&dir, lang).unwrap()))
            .collect();
        assert_eq!(after, before, "trocar o idioma de leitura mexeu em arquivo");
        assert_eq!(page_files(&dir), 30);
        assert_eq!(reading_language(&conn, "b1"), Some("pt".to_string()));
        // Voltar ao original é NULL, e não a string "original": `page_text`
        // trata os dois igual, e uma representação só é menos para lembrar.
        set_book_reading_language(&conn, "b1", Some(storage::ORIGINAL_DIR)).unwrap();
        assert_eq!(reading_language(&conn, "b1"), None);
        set_book_reading_language(&conn, "b1", None).unwrap();
        assert_eq!(reading_language(&conn, "b1"), None);
        assert_eq!(page_files(&dir), 30);
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
