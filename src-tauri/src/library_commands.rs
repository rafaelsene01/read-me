// SPEC: book-library (LIB-02, LIB-03, LIB-04, LIB-05, LIB-06, LIB-07, LIB-08,
//       LIB-09, LIB-10, LIB-11, LIB-12), book-reader (READ-13, READ-18, READ-32)

use crate::db::{require_conn, DbState};
use crate::document_commands::{unique_destination, RejectedImport};
use crate::rag::parsing::extension_of;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs::File;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

/// The only format the library accepts.
///
/// It used to be five (`pdf`, `epub`, `mobi`, `azw`, `azw3`). The reader is
/// built around EPUB - `epub-fidelity` renders the book's own HTML, CSS and
/// fonts - and the other four arrived at the reader as flat text, which is the
/// experience that feature exists to replace. Narrowed on 2026-09-08.
///
/// **Books already imported are untouched.** Nothing on the open/read/migrate
/// path asks this list: `migrate_layout` works from database rows, and a PDF
/// imported before still opens and still reprocesses.
pub const SUPPORTED_BOOK_EXTENSIONS: [&str; 1] = ["epub"];

pub fn is_supported_book(path: &Path) -> bool {
    SUPPORTED_BOOK_EXTENSIONS.contains(&extension_of(path).as_str())
}

/// `Ok(true)` protected, `Ok(false)` clean, `Err` the file could not be
/// inspected.
///
/// The three outcomes are kept apart on purpose: a file that cannot be read
/// must not be imported as if it were clean (LIB-05.3). The `Err` string is the
/// reason shown next to the file name in `RejectedImport`.
pub fn has_drm(path: &Path) -> Result<bool, String> {
    match extension_of(path).as_str() {
        "epub" => epub_has_drm(path),
        // Unreachable through the importer, which accepts nothing else. Kept as
        // a total match rather than a panic: `has_drm` is public.
        _ => Ok(false),
    }
}

fn read_error(path: &Path, e: impl std::fmt::Display) -> String {
    format!(
        "não foi possível ler {} para verificar a proteção: {e}",
        path.file_name().unwrap_or_default().to_string_lossy()
    )
}

/// EPUB is a zip; `META-INF/encryption.xml` is the DRM marker (LIB-06). An
/// archive that cannot be opened is an error, never a "clean" file.
fn epub_has_drm(path: &Path) -> Result<bool, String> {
    let file = File::open(path).map_err(|e| read_error(path, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| read_error(path, e))?;
    let entry = zip.by_name("META-INF/encryption.xml");
    match entry {
        Ok(_) => Ok(true),
        Err(zip::result::ZipError::FileNotFound) => Ok(false),
        Err(e) => Err(read_error(path, e)),
    }
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct BookRecord {
    pub id: String,
    pub filename: String,
    pub format: String,
    pub size_bytes: u64,
    pub imported_at: String,
    /// The seven reader columns of migration 10, in the order the migration
    /// creates them. `status` is a `String` and not `BookStatus` for the same
    /// reason `DocumentRecord.status` is: it comes straight out of the row.
    /// **This struct crosses the Rust/TS boundary and nothing checks the other
    /// side** (AD-054) — `src/types.ts` is hand-written and T8 is what updates
    /// it, field by field.
    pub folder: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub page_count: u32,
    pub reading_language: Option<String>,
    pub last_page: Option<u32>,
    pub last_opened_at: Option<String>,
}

/// Same contract as `ImportResult`: one bad file in a selection must not throw
/// away the good ones, and the refused ones come back named (LIB-03).
#[derive(Debug, Serialize, Clone)]
pub struct ImportBooksResult {
    pub imported: Vec<BookRecord>,
    pub rejected: Vec<RejectedImport>,
}

/// The library folder is deliberately not in `config::SUBDIRS`:
/// `ensure_folder_structure` only runs at onboarding and when the base folder
/// changes, so an install that already exists would never get the folder.
/// Creating it here is what makes LIB-11.3 hold on every path that touches the
/// library.
///
/// The path stays relative to `base_path`, which in portable mode is already
/// `./data` next to the executable (LIB-11.4).
pub(crate) fn library_dir(app: &AppHandle) -> Result<PathBuf, String> {
    // No base folder configured yet is a refusal, not an empty import (LIB-04).
    let cfg = crate::config::load_config(app)?
        .ok_or_else(|| "Nenhuma pasta de armazenamento configurada ainda".to_string())?;
    let dir = cfg.base_path_buf().join("library");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// The folder a book gets inside `library/`, and its full path (READ-32.1).
///
/// The name is the file stem run through `unique_destination`, the very same
/// disambiguation the M10.1 already applies to file names: `a.pdf` and
/// `a.epub` both want the folder `a`, so the second becomes `a (2)`
/// (READ-32.2). Deriving the name at every call site instead of storing it is
/// what `books.folder` exists to avoid.
///
/// `book_dir` has the last word on the name: it is the guard that keeps
/// `remove_book_dir` from ever being handed something that resolves to
/// `library/` itself.
fn folder_for(dir: &Path, filename: &str) -> Result<(String, PathBuf), String> {
    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = unique_destination(dir, &stem)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let path = crate::reader::storage::book_dir(dir, &name).map_err(|e| e.to_string())?;
    Ok((name, path))
}

fn row_to_book(row: &rusqlite::Row) -> rusqlite::Result<BookRecord> {
    Ok(BookRecord {
        id: row.get(0)?,
        filename: row.get(1)?,
        format: row.get(2)?,
        size_bytes: row.get::<_, i64>(3)? as u64,
        imported_at: row.get(4)?,
        folder: row.get(5)?,
        status: row.get(6)?,
        error_message: row.get(7)?,
        page_count: row.get::<_, i64>(8)? as u32,
        reading_language: row.get(9)?,
        last_page: row.get::<_, Option<i64>>(10)?.map(|page| page as u32),
        last_opened_at: row.get(11)?,
    })
}

/// The whole import, with the database and the destination folder passed in so
/// it can be exercised against an in-memory database and a temp folder — the
/// `#[tauri::command]` around it only resolves those two things.
fn import_all(conn: &Connection, dir: &Path, paths: Vec<String>) -> ImportBooksResult {
    let mut imported = Vec::new();
    let mut rejected = Vec::new();

    for raw in paths {
        let source = PathBuf::from(&raw);
        let mut reject = |reason: String| {
            rejected.push(RejectedImport {
                path: source.to_string_lossy().to_string(),
                reason,
            });
        };

        if !is_supported_book(&source) {
            reject("formato não suportado. Aceito: EPUB".to_string());
            continue;
        }
        // No size limit on purpose: without RAG an import is a single
        // `fs::copy`, and a scanned art book legitimately passes the 100 MB
        // that `document_commands` has to enforce because it embeds.
        let metadata = match std::fs::metadata(&source) {
            Ok(metadata) => metadata,
            Err(e) => {
                reject(e.to_string());
                continue;
            }
        };
        // Checked before copying: a protected book must not leave a file
        // behind in the library (LIB-05, LIB-06).
        match has_drm(&source) {
            Ok(true) => {
                reject("está protegido por DRM e não pode ser importado".to_string());
                continue;
            }
            Ok(false) => {}
            Err(e) => {
                reject(e);
                continue;
            }
        }
        let Some(filename) = source.file_name().map(|n| n.to_string_lossy().to_string()) else {
            reject("caminho de arquivo inválido".to_string());
            continue;
        };

        // The book gets a folder of its own, and the file goes inside it
        // (READ-32.1). This is the revocation of LIB-02 as written: the
        // destination is `library/<folder>/<filename>`, never `library/<filename>`.
        let (folder, book_folder) = match folder_for(dir, &filename) {
            Ok(pair) => pair,
            Err(e) => {
                reject(e);
                continue;
            }
        };
        if let Err(e) = std::fs::create_dir_all(&book_folder) {
            reject(e.to_string());
            continue;
        }
        let destination = book_folder.join(&filename);
        if let Err(e) = std::fs::copy(&source, &destination) {
            // The folder was created one line above and holds nothing else, so
            // a failed copy must not leave an empty book folder in the library.
            let _ = std::fs::remove_dir_all(&book_folder);
            reject(e.to_string());
            continue;
        }

        let record = BookRecord {
            id: Uuid::new_v4().to_string(),
            // The original name, and it no longer needs a suffix: the folder is
            // what got disambiguated, so two books named `livro.pdf` keep their
            // name and differ by folder. There is no `file_path` column — the
            // path is always `<base_path>/library/<folder>/<filename>`, and an
            // absolute path would break portable mode when the drive letter
            // changes.
            filename,
            format: extension_of(&source),
            size_bytes: metadata.len(),
            imported_at: Utc::now().to_rfc3339(),
            // The reader columns as the INSERT below leaves them: an imported
            // book has no pages until `process_book` runs.
            folder: Some(folder.clone()),
            status: "imported".to_string(),
            error_message: None,
            page_count: 0,
            reading_language: None,
            last_page: None,
            last_opened_at: None,
        };

        // No text extraction, no chunking, no embedding and no `documents`
        // row: a book is a file plus a row, nothing else (LIB-07, LIB-08).
        if let Err(e) = conn.execute(
            "INSERT INTO books (id, filename, format, size_bytes, imported_at, folder)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.id,
                record.filename,
                record.format,
                record.size_bytes as i64,
                record.imported_at,
                folder
            ],
        ) {
            // Without this the folder would sit in the library with no row,
            // invisible in the UI and impossible to remove from it.
            let _ = std::fs::remove_dir_all(&book_folder);
            reject(e.to_string());
            continue;
        }
        imported.push(record);
    }

    ImportBooksResult { imported, rejected }
}

fn select_books(conn: &Connection) -> Result<Vec<BookRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, filename, format, size_bytes, imported_at, folder, status,
                    error_message, page_count, reading_language, last_page, last_opened_at
             FROM books ORDER BY imported_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let books = stmt
        .query_map([], row_to_book)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(books)
}

/// The row goes first and the file after, and a missing file is not an error:
/// a book the user already deleted by hand must still disappear from the list
/// instead of becoming impossible to remove (LIB-10).
pub(crate) fn remove_book(conn: &Connection, dir: &Path, id: &str) -> Result<(), String> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT filename, folder FROM books WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    let deleted = conn
        .execute("DELETE FROM books WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    if deleted == 0 {
        return Err("Livro não encontrado".to_string());
    }
    match row {
        // Since READ-32 the file lives inside the book's folder, so deleting
        // only `library/<filename>` would leave the whole folder orphaned on
        // disk with no row left to find it by.
        Some((_, Some(folder))) => {
            let _ = crate::reader::storage::remove_book_dir(dir, &folder);
        }
        // `folder` NULL is the pre-READ-32 layout: the file sits loose in
        // `library/`. Reachable after boot only for a row the layout migration
        // could not move.
        Some((filename, None)) => {
            let _ = std::fs::remove_file(dir.join(filename));
        }
        None => {}
    }
    Ok(())
}

/// Moves a library still on the pre-READ-32 layout into one folder per book,
/// once, at boot (READ-32.3).
///
/// **It lives here and not in `reader::storage` even though the design lists it
/// there:** it needs a `Connection`, and `storage` is deliberately made of pure
/// functions over `&Path` so it can be exercised without a database or an
/// `AppHandle`. Path building still happens in exactly one place — `book_dir`
/// via `folder_for` — which is what centralising the layout was for.
///
/// Idempotent by construction: the candidates are the rows with `folder` NULL,
/// and a row stops being a candidate the moment its folder is recorded
/// (READ-32.4). Nothing is ever lost — a book that cannot be moved keeps both
/// its row and its file, and the reason is printed (READ-32.5).
///
/// Returns how many books were moved.
fn migrate_layout(conn: &Connection, dir: &Path) -> usize {
    let legacy: Vec<(String, String)> = {
        let Ok(mut stmt) = conn.prepare("SELECT id, filename FROM books WHERE folder IS NULL")
        else {
            return 0;
        };
        let Ok(rows) = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))) else {
            return 0;
        };
        rows.filter_map(Result::ok).collect()
    };

    let mut moved = 0;
    for (id, filename) in legacy {
        let source = dir.join(&filename);
        if !source.is_file() {
            // A library the user half-deleted by hand must not stop the app
            // from opening: the row keeps its NULL and the book shows up as
            // it did before.
            eprintln!("library: {filename} is not in the library folder, layout migration skipped it");
            continue;
        }
        let (folder, book_folder) = match folder_for(dir, &filename) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("library: no folder name for {filename}: {e}");
                continue;
            }
        };
        if let Err(e) = std::fs::create_dir_all(&book_folder) {
            eprintln!("library: could not create the folder for {filename}: {e}");
            continue;
        }
        if let Err(e) = std::fs::rename(&source, book_folder.join(&filename)) {
            // `remove_dir` and not `remove_dir_all`: the folder was created
            // three lines above and must still be empty, and refusing to
            // delete a non-empty one is the cheapest guard there is.
            let _ = std::fs::remove_dir(&book_folder);
            eprintln!("library: could not move {filename} into its folder: {e}");
            continue;
        }
        if let Err(e) = conn.execute(
            "UPDATE books SET folder = ?1 WHERE id = ?2",
            params![folder, id],
        ) {
            // The row is the only thing that knows where the file went. With
            // the update lost, the move has to be undone or the book would be
            // unreachable from the app.
            let _ = std::fs::rename(book_folder.join(&filename), &source);
            let _ = std::fs::remove_dir(&book_folder);
            eprintln!("library: could not record the folder of {filename}: {e}");
            continue;
        }
        moved += 1;
    }
    moved
}

/// The boot half of `migrate_layout`, called from `setup` next to
/// `requeue_unfinished_documents`. Silent when there is no database yet: the
/// user has not finished onboarding, so there is no library to migrate.
pub fn migrate_legacy_layout(app: &AppHandle) {
    let Ok(dir) = library_dir(app) else { return };
    let db = app.state::<DbState>();
    let Ok(guard) = db.0.lock() else { return };
    let Some(conn) = guard.as_ref() else { return };
    let moved = migrate_layout(conn, &dir);
    if moved > 0 {
        println!("library: moved {moved} book(s) into a folder of their own");
    }
}

#[tauri::command]
pub fn import_books(
    app: AppHandle,
    db: State<DbState>,
    paths: Vec<String>,
) -> Result<ImportBooksResult, String> {
    let dir = library_dir(&app)?;
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    let conn = require_conn(&guard)?;
    // The lock is held across the copies. Importing is a foreground action the
    // user just triggered and there is no background pipeline to starve — the
    // command ends when the last `fs::copy` ends.
    Ok(import_all(conn, &dir, paths))
}

#[tauri::command]
pub fn list_books(db: State<DbState>) -> Result<Vec<BookRecord>, String> {
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    select_books(require_conn(&guard)?)
}

#[tauri::command]
pub fn delete_book(app: AppHandle, db: State<DbState>, id: String) -> Result<(), String> {
    let dir = library_dir(&app)?;
    let guard = db.0.lock().map_err(|e| e.to_string())?;
    remove_book(require_conn(&guard)?, &dir, &id)
}

/// The path itself, not an "open the folder" command: the UI has to show it
/// anyway (LIB-12) and opens it with `openPath()` from the opener plugin, so
/// one command serves both requirements (LIB-11).
#[tauri::command]
pub fn library_path(app: AppHandle) -> Result<String, String> {
    Ok(library_dir(&app)?.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("readme-library-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn epub(path: &Path, entries: &[&str]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for name in entries {
            writer.start_file::<_, ()>(*name, opts).unwrap();
            writer.write_all(b"x").unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn epub_is_accepted_whatever_the_case_of_its_extension() {
        for name in ["a.epub", "A.EPUB", "a.EpUb"] {
            assert!(is_supported_book(Path::new(name)), "recusou {name}");
        }
    }

    #[test]
    fn every_other_format_is_refused_including_the_four_that_used_to_pass() {
        // `pdf`, `mobi`, `azw` and `azw3` were accepted until 2026-09-08; the
        // reader is built around EPUB and they arrived as flat text. `.docx` is
        // accepted by the RAG importer and must not leak in here; `.kfx` never
        // was, because no open library reads it.
        for name in [
            "a.pdf", "a.mobi", "a.azw", "a.azw3", "a.docx", "a.kfx", "a.txt", "a.md",
            "no-extension",
        ] {
            assert!(!is_supported_book(Path::new(name)), "aceitou {name}");
        }
    }

    #[test]
    fn an_epub_with_encryption_xml_is_refused() {
        let path = temp_dir("epub-drm").join("livro.epub");
        epub(&path, &["mimetype", "META-INF/encryption.xml", "OEBPS/c1.html"]);
        assert_eq!(has_drm(&path), Ok(true));
    }

    #[test]
    fn an_epub_without_encryption_xml_passes() {
        let path = temp_dir("epub-clean").join("livro.epub");
        epub(&path, &["mimetype", "META-INF/container.xml", "OEBPS/c1.html"]);
        assert_eq!(has_drm(&path), Ok(false));
    }

    #[test]
    fn an_epub_that_is_not_a_zip_is_a_read_error() {
        let path = temp_dir("epub-broken").join("quebrado.epub");
        std::fs::write(&path, b"isto nao e um zip").unwrap();
        assert!(has_drm(&path).is_err());
    }

    fn migrated() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::apply_migrations(&mut conn).unwrap();
        conn
    }

    fn insert_book(conn: &rusqlite::Connection, id: &str, imported_at: &str) {
        conn.execute(
            "INSERT INTO books (id, filename, format, size_bytes, imported_at)
             VALUES (?1, ?2, 'pdf', 1, ?3)",
            params![id, format!("{id}.pdf"), imported_at],
        )
        .unwrap();
    }

    /// The folder recorded for a book, read straight from the row. `BookRecord`
    /// does **not** carry it: adding a field to a struct that crosses the
    /// Rust/TS boundary is T8's job, and AD-054 says nothing would warn about a
    /// divergence.
    fn folder_of(conn: &rusqlite::Connection, id: &str) -> Option<String> {
        conn.query_row("SELECT folder FROM books WHERE id = ?1", params![id], |row| {
            row.get(0)
        })
        .unwrap()
    }

    /// A fresh, empty folder per test: reusing one would let the collision
    /// test see leftovers from a previous run and pass for the wrong reason.
    fn empty_dir(tag: &str) -> PathBuf {
        let dir = temp_dir(tag);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn books_are_listed_from_the_newest_to_the_oldest() {
        // LIB-09. Timestamps are written explicitly instead of relying on
        // `Utc::now()`: two imports in the same run can land in the same
        // millisecond and the ordering would be undefined, not DESC.
        let conn = migrated();
        insert_book(&conn, "meio", "2026-02-02T00:00:00+00:00");
        insert_book(&conn, "velho", "2026-01-01T00:00:00+00:00");
        insert_book(&conn, "novo", "2026-03-03T00:00:00+00:00");

        let ids: Vec<String> = select_books(&conn)
            .unwrap()
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(ids, vec!["novo", "meio", "velho"]);
    }

    #[test]
    fn removing_a_book_whose_file_is_already_gone_still_drops_the_row() {
        // LIB-10: a file deleted by hand outside the app must not make the
        // book impossible to remove from the list.
        let conn = migrated();
        let dir = empty_dir("delete-missing");
        insert_book(&conn, "sumiu", "2026-01-01T00:00:00+00:00");
        assert!(!dir.join("sumiu.pdf").exists());

        assert_eq!(remove_book(&conn, &dir, "sumiu"), Ok(()));
        assert!(select_books(&conn).unwrap().is_empty());
    }

    #[test]
    fn removing_a_book_deletes_the_file_too() {
        let conn = migrated();
        let dir = empty_dir("delete-file");
        std::fs::write(dir.join("presente.pdf"), b"x").unwrap();
        conn.execute(
            "INSERT INTO books (id, filename, format, size_bytes, imported_at)
             VALUES ('presente', 'presente.pdf', 'pdf', 1, '2026-01-01T00:00:00+00:00')",
            [],
        )
        .unwrap();

        assert_eq!(remove_book(&conn, &dir, "presente"), Ok(()));
        assert!(!dir.join("presente.pdf").exists());
    }

    #[test]
    fn a_second_book_with_the_same_name_gets_a_suffix_instead_of_overwriting() {
        // LIB-02, rewritten by READ-32: what takes the `(2)` suffix is now the
        // FOLDER, not the file, and the file keeps the name the user gave it.
        // The criterion this test defends is unchanged and is the reason it was
        // updated instead of deleted — the first book's bytes must survive.
        let conn = migrated();
        let dir = empty_dir("collision");
        let source_dir = empty_dir("collision-src");
        let first = source_dir.join("livro.epub");
        epub(&first, &["mimetype", "META-INF/container.xml"]);

        let result = import_all(
            &conn,
            &dir,
            vec![first.to_string_lossy().to_string()],
        );
        assert_eq!(result.imported.len(), 1, "{:?}", result.rejected);

        // The same name, different bytes: the second archive carries one more
        // entry, which is what makes the byte comparison below meaningful.
        epub(&first, &["mimetype", "META-INF/container.xml", "OEBPS/content.opf"]);
        let result = import_all(
            &conn,
            &dir,
            vec![first.to_string_lossy().to_string()],
        );
        assert_eq!(result.imported.len(), 1, "{:?}", result.rejected);
        assert_eq!(result.imported[0].filename, "livro.epub");
        assert_eq!(folder_of(&conn, &result.imported[0].id).as_deref(), Some("livro (2)"));

        assert_ne!(
            std::fs::read(dir.join("livro").join("livro.epub")).unwrap(),
            std::fs::read(dir.join("livro (2)").join("livro.epub")).unwrap(),
            "o segundo import sobrescreveu os bytes do primeiro"
        );

        // Two rows with the same file name is now normal and no longer
        // ambiguous: the folder is what tells them apart.
        let names: Vec<String> = select_books(&conn)
            .unwrap()
            .into_iter()
            .map(|b| b.filename)
            .collect();
        assert_eq!(names, vec!["livro.epub", "livro.epub"], "{names:?}");
    }

    #[test]
    fn importing_a_book_writes_to_books_and_never_to_documents() {
        // LIB-07 and LIB-08: no RAG side effect at all. This asserts the
        // absence of a `documents` row, which is the only part of "no
        // chunking, no embedding, no LanceDB" a unit test can observe — the
        // pipeline is never called here, so nothing else could run.
        let conn = migrated();
        let dir = empty_dir("no-documents");
        let source = empty_dir("no-documents-src").join("livro.epub");
        epub(&source, &["mimetype", "META-INF/container.xml"]);

        let result = import_all(&conn, &dir, vec![source.to_string_lossy().to_string()]);
        assert_eq!(result.imported.len(), 1, "{:?}", result.rejected);
        assert_eq!(result.imported[0].format, "epub");
        assert_eq!(
            result.imported[0].size_bytes,
            std::fs::metadata(&source).unwrap().len()
        );

        let books: i64 = conn
            .query_row("SELECT COUNT(*) FROM books", [], |row| row.get(0))
            .unwrap();
        assert_eq!(books, 1);

        let documents: i64 = conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .unwrap();
        assert_eq!(documents, 0, "um livro não pode virar documento de RAG");
    }

    #[test]
    fn a_mixed_selection_keeps_the_valid_files_and_names_the_refused_ones() {
        // LIB-03: each file judged on its own. One valid EPUB, one .docx that
        // the RAG importer accepts but the library must not, and one DRM'd
        // EPUB. Only the first is imported, and the other two come back named.
        // The refused one used to be a MOBI; since 2026-09-08 a MOBI never
        // reaches the DRM check at all, so proving LIB-05 needs a protected
        // file in the one format that does.
        let conn = migrated();
        let dir = empty_dir("mixed");
        let src = empty_dir("mixed-src");

        let good = src.join("bom.epub");
        epub(&good, &["mimetype", "META-INF/container.xml"]);
        let wrong_format = src.join("texto.docx");
        std::fs::write(&wrong_format, b"conteudo").unwrap();
        let protected = src.join("protegido.epub");
        epub(
            &protected,
            &["mimetype", "META-INF/container.xml", "META-INF/encryption.xml"],
        );

        let result = import_all(
            &conn,
            &dir,
            vec![
                good.to_string_lossy().to_string(),
                wrong_format.to_string_lossy().to_string(),
                protected.to_string_lossy().to_string(),
            ],
        );

        assert_eq!(result.imported.len(), 1);
        assert_eq!(result.imported[0].filename, "bom.epub");
        assert_eq!(result.rejected.len(), 2);
        assert!(result.rejected[0].reason.contains("não suportado"));
        assert!(result.rejected[1].reason.contains("DRM"));

        // The DRM'd file is refused before the copy: nothing of it may land
        // in the library folder (LIB-05). Not even a folder — the check comes
        // before `folder_for`.
        assert!(!dir.join("protegido").exists());
        assert!(!dir.join("texto").exists());
        assert!(dir.join("bom").join("bom.epub").is_file());
        assert_eq!(select_books(&conn).unwrap().len(), 1);
    }

    #[test]
    fn importing_puts_the_file_inside_a_folder_of_its_own() {
        // READ-32.1, and the revocation of LIB-02 as written: the destination
        // is `library/<folder>/<file>`, with nothing left loose in the root.
        let conn = migrated();
        let dir = empty_dir("own-folder");
        let source = empty_dir("own-folder-src").join("livro.epub");
        epub(&source, &["mimetype", "META-INF/container.xml"]);

        let result = import_all(&conn, &dir, vec![source.to_string_lossy().to_string()]);
        assert_eq!(result.imported.len(), 1, "{:?}", result.rejected);

        assert!(
            !dir.join("livro.epub").exists(),
            "o arquivo continuou solto na raiz da biblioteca"
        );
        assert!(dir.join("livro").join("livro.epub").is_file());
        assert_eq!(result.imported[0].filename, "livro.epub");
        assert_eq!(
            folder_of(&conn, &result.imported[0].id).as_deref(),
            Some("livro"),
            "a pasta não foi gravada na linha"
        );
    }

    #[test]
    fn two_books_that_would_share_a_folder_name_get_a_suffix() {
        // READ-32.2. Two files named `a.epub`, from two different folders,
        // both want the folder `a`. This is the case the design uses to justify
        // storing the folder name instead of deriving it from the file name.
        // It used to be `a.pdf` + `a.epub`; with EPUB the only accepted format,
        // the clash now comes from two sources rather than two extensions.
        let conn = migrated();
        let dir = empty_dir("folder-clash");
        let first_src = empty_dir("folder-clash-src-1");
        let second_src = empty_dir("folder-clash-src-2");
        let first = first_src.join("a.epub");
        epub(&first, &["mimetype", "META-INF/container.xml"]);
        let second = second_src.join("a.epub");
        epub(&second, &["mimetype", "OEBPS/content.opf"]);

        let result = import_all(
            &conn,
            &dir,
            vec![
                first.to_string_lossy().to_string(),
                second.to_string_lossy().to_string(),
            ],
        );
        assert_eq!(result.imported.len(), 2, "{:?}", result.rejected);

        let folders: Vec<Option<String>> = result
            .imported
            .iter()
            .map(|b| folder_of(&conn, &b.id))
            .collect();
        assert_eq!(
            folders,
            vec![Some("a".to_string()), Some("a (2)".to_string())]
        );
        assert!(dir.join("a").join("a.epub").is_file());
        assert!(dir.join("a (2)").join("a.epub").is_file());
        // The two files are different archives, so this also proves the second
        // import did not overwrite the first.
        assert_ne!(
            std::fs::read(dir.join("a").join("a.epub")).unwrap(),
            std::fs::read(dir.join("a (2)").join("a.epub")).unwrap(),
            "o segundo livro sobrescreveu o primeiro"
        );
    }

    #[test]
    fn the_layout_migration_moves_a_loose_file_into_its_folder_and_records_it() {
        // READ-32.3. An M10.1 library: the file sits loose in `library/` and
        // the row has a NULL `folder`.
        let conn = migrated();
        let dir = empty_dir("legacy");
        insert_book(&conn, "solto", "2026-01-01T00:00:00+00:00");
        std::fs::write(dir.join("solto.pdf"), b"conteudo").unwrap();
        assert_eq!(folder_of(&conn, "solto"), None);

        assert_eq!(migrate_layout(&conn, &dir), 1);

        assert!(!dir.join("solto.pdf").exists(), "o arquivo não saiu da raiz");
        assert_eq!(
            std::fs::read(dir.join("solto").join("solto.pdf")).unwrap(),
            b"conteudo",
            "o conteúdo mudou na mudança"
        );
        assert_eq!(folder_of(&conn, "solto").as_deref(), Some("solto"));
        // The row stays listable: migrating must not make the book disappear.
        assert_eq!(select_books(&conn).unwrap().len(), 1);
    }

    #[test]
    fn running_the_layout_migration_twice_moves_nothing_the_second_time() {
        // READ-32.4. Idempotent by construction: the candidates are the rows
        // with a NULL `folder`, and recording the folder takes a row off that
        // list for good.
        let conn = migrated();
        let dir = empty_dir("legacy-twice");
        insert_book(&conn, "solto", "2026-01-01T00:00:00+00:00");
        std::fs::write(dir.join("solto.pdf"), b"conteudo").unwrap();
        assert_eq!(migrate_layout(&conn, &dir), 1);

        // Text already processed inside the folder: if the second pass touched
        // anything, this is what it would destroy.
        let original = dir.join("solto").join("original");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::write(original.join("0001.txt"), b"pagina 1").unwrap();

        assert_eq!(migrate_layout(&conn, &dir), 0, "moveu de novo");

        assert!(dir.join("solto").join("solto.pdf").is_file());
        assert_eq!(
            std::fs::read(original.join("0001.txt")).unwrap(),
            b"pagina 1"
        );
        assert!(
            !dir.join("solto (2)").exists(),
            "criou uma segunda pasta para o mesmo livro"
        );
        assert_eq!(folder_of(&conn, "solto").as_deref(), Some("solto"));
    }

    #[test]
    fn a_file_that_cannot_be_moved_keeps_its_row_and_its_file() {
        // READ-32.5. The failure is INJECTED through a `filename` carrying a
        // path separator: the destination becomes `library/livro/sub/livro.pdf`,
        // whose parent directory does not exist, and `fs::rename` fails with
        // NotFound.
        //
        // INCONCLUSIVE ABOUT THE REAL-WORLD CAUSE: the move failures that
        // actually happen on a user's machine are permission denied and file in
        // use, and neither is portably reproducible in a test. What this test
        // proves is the error BRANCH -- the row and the file are left exactly as
        // they were, and no empty folder is left behind -- not that this is the
        // condition that triggers it in practice.
        let conn = migrated();
        let dir = empty_dir("legacy-stuck");
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("livro.pdf"), b"intacto").unwrap();
        conn.execute(
            "INSERT INTO books (id, filename, format, size_bytes, imported_at)
             VALUES ('preso', 'sub/livro.pdf', 'pdf', 7, '2026-01-01T00:00:00+00:00')",
            [],
        )
        .unwrap();

        assert_eq!(migrate_layout(&conn, &dir), 0);

        // Nothing is lost: the row is still there without a folder, and so is
        // the file.
        assert_eq!(folder_of(&conn, "preso"), None);
        assert_eq!(select_books(&conn).unwrap().len(), 1);
        assert_eq!(std::fs::read(sub.join("livro.pdf")).unwrap(), b"intacto");
        assert!(
            !dir.join("livro").exists(),
            "sobrou uma pasta vazia da tentativa que falhou"
        );
    }

    #[test]
    fn a_row_whose_file_is_already_gone_is_skipped_without_failing_the_boot() {
        // A library the user half-deleted by hand must not stop the app from
        // opening: the orphan row is skipped and the books that still have a
        // file are migrated.
        let conn = migrated();
        let dir = empty_dir("legacy-halfgone");
        insert_book(&conn, "sumiu", "2026-01-01T00:00:00+00:00");
        insert_book(&conn, "existe", "2026-02-02T00:00:00+00:00");
        std::fs::write(dir.join("existe.pdf"), b"aqui").unwrap();

        assert_eq!(migrate_layout(&conn, &dir), 1);

        assert_eq!(
            folder_of(&conn, "sumiu"),
            None,
            "inventou pasta para arquivo ausente"
        );
        assert!(!dir.join("sumiu").exists());
        assert_eq!(folder_of(&conn, "existe").as_deref(), Some("existe"));
        assert!(dir.join("existe").join("existe.pdf").is_file());
        // Both rows are still listed: skipping is not deleting.
        assert_eq!(select_books(&conn).unwrap().len(), 2);
    }

    #[test]
    fn removing_a_book_deletes_its_folder_and_everything_in_it() {
        // A direct consequence of READ-32: with the file inside the folder,
        // deleting only `library/<file>` would leave the whole folder orphaned
        // on disk, with no row left to find it by.
        let conn = migrated();
        let dir = empty_dir("delete-folder");
        let source = empty_dir("delete-folder-src").join("livro.epub");
        epub(&source, &["mimetype", "META-INF/container.xml"]);
        let result = import_all(&conn, &dir, vec![source.to_string_lossy().to_string()]);
        let id = result.imported[0].id.clone();
        let pt = dir.join("livro").join("pt");
        std::fs::create_dir_all(&pt).unwrap();
        std::fs::write(pt.join("0001.txt"), b"traducao").unwrap();

        assert_eq!(remove_book(&conn, &dir, &id), Ok(()));

        assert!(!dir.join("livro").exists(), "a pasta do livro sobreviveu");
        assert!(select_books(&conn).unwrap().is_empty());
    }

    /// How many files live under `dir`, at any depth. It is the number the
    /// real-library rehearsal compares before and after.
    fn count_files(dir: &Path) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .map(|e| {
                let path = e.path();
                if path.is_dir() {
                    count_files(&path)
                } else {
                    1
                }
            })
            .sum()
    }

    /// The rehearsal `AGENTS.md` demands before a destructive migration counts
    /// as done, in the second `#[ignore]` format this codebase uses: the path
    /// comes from an environment variable and is **never** guessed -- the same
    /// shape as `db::real_database` and `runtime::process::sidecar_real`.
    ///
    /// WARNING: `READER_LEGACY_LIBRARY` must point at a **COPY** of a
    /// `library/` folder. This test MOVES every book file it finds there. The
    /// user's real library is never opened for writing.
    ///
    /// ```text
    /// cargo test --lib migrate_legacy_layout_against_a_real_library_copy -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "requer READER_LEGACY_LIBRARY apontando para uma COPIA de biblioteca"]
    fn migrate_legacy_layout_against_a_real_library_copy() {
        let raw = std::env::var("READER_LEGACY_LIBRARY")
            .expect("defina READER_LEGACY_LIBRARY com o caminho de uma COPIA da biblioteca");
        let dir = PathBuf::from(raw);
        assert!(dir.is_dir(), "{} não é uma pasta", dir.display());

        // Only loose files in a supported format become rows: anything else in
        // the folder is not a candidate and is never touched.
        let mut loose: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_file() && is_supported_book(&e.path()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        loose.sort();

        let files_before = count_files(&dir);
        let conn = migrated();
        for (i, name) in loose.iter().enumerate() {
            conn.execute(
                "INSERT INTO books (id, filename, format, size_bytes, imported_at)
                 VALUES (?1, ?2, ?3, 0, '2026-01-01T00:00:00+00:00')",
                params![format!("real-{i}"), name, extension_of(Path::new(name))],
            )
            .unwrap();
        }

        let moved = migrate_layout(&conn, &dir);
        let files_after = count_files(&dir);
        println!(
            "READER_LEGACY_LIBRARY={}: {} arquivos antes, {} depois, {} de {} livros movidos",
            dir.display(),
            files_before,
            files_after,
            moved,
            loose.len()
        );

        assert_eq!(
            files_before, files_after,
            "a migração perdeu ou duplicou arquivo"
        );
        assert_eq!(moved, loose.len(), "algum livro não foi movido");
        for (i, name) in loose.iter().enumerate() {
            let folder = folder_of(&conn, &format!("real-{i}")).expect("pasta não gravada");
            assert!(
                dir.join(&folder).join(name).is_file(),
                "{name} não está em {folder}"
            );
            assert!(!dir.join(name).exists(), "{name} continuou solto");
        }

        // Second pass: idempotence against real data.
        assert_eq!(migrate_layout(&conn, &dir), 0);
        assert_eq!(count_files(&dir), files_after);
    }
}
