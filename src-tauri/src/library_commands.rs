// SPEC: book-library (LIB-02, LIB-03, LIB-04, LIB-05, LIB-06, LIB-07, LIB-08,
//       LIB-09, LIB-10, LIB-11, LIB-12)

use crate::db::{require_conn, DbState};
use crate::document_commands::{unique_destination, RejectedImport};
use crate::rag::parsing::extension_of;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
use uuid::Uuid;

/// The five formats the library accepts. `.kfx` is deliberately absent: no open
/// library reads it reliably, so importing one would produce a book that the
/// reader can never open.
pub const SUPPORTED_BOOK_EXTENSIONS: [&str; 5] = ["pdf", "epub", "mobi", "azw", "azw3"];

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
        "mobi" | "azw" | "azw3" => palmdb_has_drm(path),
        // PDF encryption is out of scope for this feature; saying "clean" here
        // is a scope decision, not a measurement.
        _ => Ok(false),
    }
}

fn read_error(path: &Path, e: impl std::fmt::Display) -> String {
    format!(
        "não foi possível ler {} para verificar a proteção: {e}",
        path.file_name().unwrap_or_default().to_string_lossy()
    )
}

/// PalmDB (`.mobi`, `.azw`, `.azw3`): the record info list starts at byte 78 and
/// each entry is 8 bytes, the first 4 holding the record's absolute offset in
/// the file. Record 0 is the PalmDOC header, whose bytes 12..14 are the
/// encryption type as a big-endian u16 — 0 none, 1 legacy, 2 Mobipocket DRM.
/// Anything non-zero is a refusal (LIB-05).
fn palmdb_has_drm(path: &Path) -> Result<bool, String> {
    let mut file = File::open(path).map_err(|e| read_error(path, e))?;

    let mut offset = [0u8; 4];
    file.seek(SeekFrom::Start(78))
        .map_err(|e| read_error(path, e))?;
    file.read_exact(&mut offset)
        .map_err(|e| read_error(path, e))?;

    // Measured while writing the tests: a file of 86 zero bytes reads its
    // record-0 offset as 0, and reading "bytes 12..14 of record 0" would land
    // inside the fixed 78-byte header — all zeros, i.e. a false "clean file".
    // A record cannot start inside the header, so that is a broken file.
    let record0 = u32::from_be_bytes(offset) as u64;
    if record0 < 78 {
        return Err(read_error(
            path,
            "cabeçalho PalmDB inválido: o registro 0 aponta para dentro do cabeçalho",
        ));
    }

    let mut encryption = [0u8; 2];
    file.seek(SeekFrom::Start(record0 + 12))
        .map_err(|e| read_error(path, e))?;
    file.read_exact(&mut encryption)
        .map_err(|e| read_error(path, e))?;

    Ok(u16::from_be_bytes(encryption) != 0)
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
fn library_dir(app: &AppHandle) -> Result<PathBuf, String> {
    // No base folder configured yet is a refusal, not an empty import (LIB-04).
    let cfg = crate::config::load_config(app)?
        .ok_or_else(|| "Nenhuma pasta de armazenamento configurada ainda".to_string())?;
    let dir = cfg.base_path_buf().join("library");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn row_to_book(row: &rusqlite::Row) -> rusqlite::Result<BookRecord> {
    Ok(BookRecord {
        id: row.get(0)?,
        filename: row.get(1)?,
        format: row.get(2)?,
        size_bytes: row.get::<_, i64>(3)? as u64,
        imported_at: row.get(4)?,
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
            reject("formato não suportado. Aceitos: PDF, EPUB, MOBI, AZW, AZW3".to_string());
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

        let destination = unique_destination(dir, &filename);
        if let Err(e) = std::fs::copy(&source, &destination) {
            reject(e.to_string());
            continue;
        }

        let record = BookRecord {
            id: Uuid::new_v4().to_string(),
            // The name on disk, not the original one: a collision renamed it,
            // and the row must point at the file that actually exists. There
            // is no `file_path` column — the path is always
            // `<base_path>/library/<filename>`, and an absolute path would
            // break portable mode when the drive letter changes.
            filename: destination
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or(filename),
            format: extension_of(&source),
            size_bytes: metadata.len(),
            imported_at: Utc::now().to_rfc3339(),
        };

        // No text extraction, no chunking, no embedding and no `documents`
        // row: a book is a file plus a row, nothing else (LIB-07, LIB-08).
        if let Err(e) = conn.execute(
            "INSERT INTO books (id, filename, format, size_bytes, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.id,
                record.filename,
                record.format,
                record.size_bytes as i64,
                record.imported_at
            ],
        ) {
            // Without this the file would sit in the library with no row,
            // invisible in the UI and impossible to remove from it.
            let _ = std::fs::remove_file(&destination);
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
            "SELECT id, filename, format, size_bytes, imported_at
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
fn remove_book(conn: &Connection, dir: &Path, id: &str) -> Result<(), String> {
    let filename: Option<String> = conn
        .query_row(
            "SELECT filename FROM books WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .ok();
    let deleted = conn
        .execute("DELETE FROM books WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    if deleted == 0 {
        return Err("Livro não encontrado".to_string());
    }
    if let Some(filename) = filename {
        let _ = std::fs::remove_file(dir.join(filename));
    }
    Ok(())
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

    /// A minimal PalmDB: 78-byte header, one record info entry pointing at
    /// record 0, and a record 0 long enough to carry the encryption field.
    fn palmdb(path: &Path, encryption: u16) {
        let record0_offset: u32 = 78 + 8;
        let mut bytes = vec![0u8; 78];
        bytes[76..78].copy_from_slice(&1u16.to_be_bytes()); // numRecords
        bytes.extend_from_slice(&record0_offset.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 4]); // attributes + uniqueID
        let mut record0 = vec![0u8; 16];
        record0[12..14].copy_from_slice(&encryption.to_be_bytes());
        bytes.extend_from_slice(&record0);
        std::fs::write(path, bytes).unwrap();
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
    fn the_five_book_formats_are_accepted() {
        for name in ["a.pdf", "a.epub", "a.mobi", "a.azw", "a.azw3", "A.EPUB"] {
            assert!(is_supported_book(Path::new(name)), "recusou {name}");
        }
    }

    #[test]
    fn other_formats_are_refused() {
        // .docx is accepted by the RAG importer and must NOT leak into the
        // library; .kfx is refused on purpose (no reader can open it).
        for name in ["a.docx", "a.kfx", "a.txt", "a.md", "no-extension"] {
            assert!(!is_supported_book(Path::new(name)), "aceitou {name}");
        }
    }

    #[test]
    fn a_palmdb_without_encryption_passes() {
        let path = temp_dir("clean").join("livro.mobi");
        palmdb(&path, 0);
        assert_eq!(has_drm(&path), Ok(false));
    }

    #[test]
    fn a_palmdb_with_a_non_zero_encryption_field_is_refused() {
        // 1 = legacy Mobipocket encryption, 2 = Mobipocket DRM. Both refuse.
        for (encryption, ext) in [(1u16, "azw"), (2u16, "azw3")] {
            let path = temp_dir("drm").join(format!("livro-{encryption}.{ext}"));
            palmdb(&path, encryption);
            assert_eq!(has_drm(&path), Ok(true), "campo {encryption} passou");
        }
    }

    #[test]
    fn a_truncated_palmdb_is_a_read_error_not_a_clean_file() {
        // The dangerous inverse: a file that cannot be inspected must never be
        // reported as DRM-free (LIB-05.3).
        let dir = temp_dir("truncated");
        let short = dir.join("cortado.mobi");
        std::fs::write(&short, vec![0u8; 40]).unwrap();
        assert!(has_drm(&short).is_err(), "header curto virou 'sem DRM'");

        // 86 zero bytes: long enough to read the record-0 offset, which comes
        // out as 0. The first version of this function seeked to 0+12 and read
        // header padding as the encryption field, returning Ok(false) — a
        // corrupt file reported as clean. This assertion is why the guard
        // exists.
        let zeroed = dir.join("sem-registro.azw");
        std::fs::write(&zeroed, vec![0u8; 86]).unwrap();
        assert!(has_drm(&zeroed).is_err(), "registro 0 inválido virou 'sem DRM'");

        // Offset points past the end of the file: only the read can catch it.
        let mut past_eof = vec![0u8; 86];
        past_eof[78..82].copy_from_slice(&4096u32.to_be_bytes());
        let headless = dir.join("registro-fora.azw3");
        std::fs::write(&headless, past_eof).unwrap();
        assert!(has_drm(&headless).is_err(), "registro 0 ausente virou 'sem DRM'");

        let missing = dir.join("nao-existe.azw3");
        assert!(has_drm(&missing).is_err());
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
        // LIB-02. Two different files that happen to share a name: the first
        // one's bytes must survive.
        let conn = migrated();
        let dir = empty_dir("collision");
        let source_dir = empty_dir("collision-src");
        let first = source_dir.join("livro.pdf");
        std::fs::write(&first, b"primeiro").unwrap();

        let result = import_all(
            &conn,
            &dir,
            vec![first.to_string_lossy().to_string()],
        );
        assert_eq!(result.imported.len(), 1);

        std::fs::write(&first, b"segundo").unwrap();
        let result = import_all(
            &conn,
            &dir,
            vec![first.to_string_lossy().to_string()],
        );
        assert_eq!(result.imported.len(), 1);
        assert_eq!(result.imported[0].filename, "livro (2).pdf");

        assert_eq!(std::fs::read(dir.join("livro.pdf")).unwrap(), b"primeiro");
        assert_eq!(std::fs::read(dir.join("livro (2).pdf")).unwrap(), b"segundo");

        // The row must name the file that actually exists, otherwise the
        // library would list a book it can never open or delete.
        let names: Vec<String> = select_books(&conn)
            .unwrap()
            .into_iter()
            .map(|b| b.filename)
            .collect();
        assert!(names.contains(&"livro (2).pdf".to_string()), "{names:?}");
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
        // MOBI. Only the first is imported, and the other two come back named.
        let conn = migrated();
        let dir = empty_dir("mixed");
        let src = empty_dir("mixed-src");

        let good = src.join("bom.epub");
        epub(&good, &["mimetype", "META-INF/container.xml"]);
        let wrong_format = src.join("texto.docx");
        std::fs::write(&wrong_format, b"conteudo").unwrap();
        let protected = src.join("protegido.mobi");
        palmdb(&protected, 2);

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
        // in the library folder (LIB-05).
        assert!(!dir.join("protegido.mobi").exists());
        assert!(!dir.join("texto.docx").exists());
        assert!(dir.join("bom.epub").exists());
        assert_eq!(select_books(&conn).unwrap().len(), 1);
    }

    #[test]
    fn a_pdf_is_never_inspected() {
        // INCONCLUSIVE AS PROOF OF ANYTHING ABOUT PDF DRM: this fixes the
        // Out of Scope decision (PDF encryption is not checked in this
        // feature), not that the file is unprotected. An encrypted PDF passes
        // here by design.
        let path = temp_dir("pdf").join("livro.pdf");
        std::fs::write(&path, b"%PDF-1.7 whatever").unwrap();
        assert_eq!(has_drm(&path), Ok(false));
    }
}
