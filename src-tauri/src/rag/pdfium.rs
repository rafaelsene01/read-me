// SPEC: self-contained-runtime (SELF-12), documents-rag (DOC-04), book-reader (READ-08),
//       book-illustrations (ILLUS-02, ILLUS-08)

use crate::reader::illustrations::{self, Illustration};
use crate::runtime::bundled;
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;

/// pdfium replaced `pdf-extract` 0.12, which silently dropped whole glyphs from
/// this project's own corpus: `q`, `v`, `x`, `b`, `f` and every accented vowel
/// vanished from 51% of the chunks of a Código Civil PDF ("salvo se o exercício
/// da profissão" came out as "salo se o eerccio da profisso"). pdfium reads the
/// same file with zero losses, measured against poppler as a reference.
///
/// It used to be downloaded on first use; it now ships in the installer, and
/// the pinned release lives in `scripts/vendor.json`.

/// Set once the library is resolved; `extract_text` is synchronous and has no
/// `AppHandle`, so the path has to outlive the lookup. Same shape as
/// `embedding::MODEL_CACHE_DIR`.
static LIBRARY_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Built once and reused. `Pdfium::bind_to_library` returns
/// `PdfiumLibraryBindingsAlreadyInitialized` on the second call in a process -
/// the crate parks its bindings in a global `OnceCell` and `Pdfium::new` asserts
/// that cell is empty - so binding per extraction breaks the SECOND PDF of every
/// run. Measured against the vendored pdfium.dll, not inferred from the docs.
static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

/// pdfium is not thread-safe, and the crate's `thread_safe` feature only adds
/// `unsafe impl Send`/`Sync` - it serializes nothing. Indexing a document and
/// processing a book can reach `extract_text` at the same time, so this lock is
/// what keeps two callers out of pdfium together.
/// ponytail: one global lock, so PDF extractions never overlap; make it
/// finer-grained only if a measurement shows the serialization hurting.
static EXTRACTING: Mutex<()> = Mutex::new(());

/// Only PDFs need the library, so importing a `.txt` never touches it.
pub async fn ensure_for(app: &AppHandle, path: &Path) -> Result<(), String> {
    if super::parsing::extension_of(path) != "pdf" {
        return Ok(());
    }
    ensure_library(app).await.map(|_| ())
}

/// Still `async` because the document pipeline calls it that way; there is no
/// I/O left to await beyond resolving a path.
pub async fn ensure_library(app: &AppHandle) -> Result<PathBuf, String> {
    let library = bundled::pdfium_library(app)?;
    remember(&library);
    Ok(library)
}

fn remember(path: &Path) {
    if let Ok(mut current) = LIBRARY_PATH.lock() {
        *current = Some(path.to_path_buf());
    }
}

/// Concatenates every page's text.
pub fn extract_text(pdf: &Path) -> Result<String, String> {
    extract(pdf, false).map(|(text, _)| text)
}

/// The same walk, keeping the pictures (ILLUS-02).
///
/// Every image object of a page that clears
/// [`illustrations::MIN_IMAGE_SIDE_PX`] is re-encoded as a PNG and gets a
/// marker paragraph. An object that cannot be decoded is dropped along with
/// its marker (ILLUS-08) - one bad picture must not cost the user the book.
///
/// ponytail: the markers of a page are appended **after** that page's text,
/// not interleaved with it. pdfium exposes text and images as separate object
/// lists, and threading them by their bounding boxes is a layout problem
/// nobody has asked to solve; if T7 shows figures landing far from their
/// paragraph, sort by `object.bounds()` here.
pub fn extract_text_and_images(pdf: &Path) -> Result<(String, Vec<Illustration>), String> {
    extract(pdf, true)
}

fn extract(pdf: &Path, want_images: bool) -> Result<(String, Vec<Illustration>), String> {
    let library = LIBRARY_PATH
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "o leitor de PDF ainda não foi carregado".to_string())?;

    // A panic inside pdfium must not poison the reader for the rest of the
    // session: the data behind this lock is `()`, so there is no broken state
    // to protect from the next caller.
    let _serialized = EXTRACTING.lock().unwrap_or_else(|e| e.into_inner());

    let pdfium = match PDFIUM.get() {
        Some(pdfium) => pdfium,
        None => {
            let bindings = Pdfium::bind_to_library(&library)
                .map_err(|e| format!("não foi possível carregar o pdfium: {e}"))?;
            // The lock above is what makes this the only initialization in
            // flight, so `Pdfium::new`'s internal assert cannot lose a race.
            let _ = PDFIUM.set(Pdfium::new(bindings));
            PDFIUM
                .get()
                .ok_or_else(|| "não foi possível inicializar o pdfium".to_string())?
        }
    };

    let document = pdfium
        .load_pdf_from_file(pdf, None)
        .map_err(|e| format!("não foi possível abrir o PDF: {e}"))?;

    let mut out = String::new();
    let mut images: Vec<Illustration> = Vec::new();
    for page in document.pages().iter() {
        let text = page
            .text()
            .map_err(|e| format!("não foi possível ler o texto da página: {e}"))?;
        out.push_str(&text.all());
        out.push('\n');
        if !want_images {
            continue;
        }
        for object in page.objects().iter() {
            let Some(image) = object.as_image_object() else {
                continue;
            };
            // Every failure below is a dropped picture, never an error: a rule,
            // a bullet, or an object pdfium cannot decode is not worth the
            // book (ILLUS-08).
            let Ok(raw) = image.get_raw_image() else {
                continue;
            };
            if !illustrations::is_large_enough(raw.width(), raw.height()) {
                continue;
            }
            let mut png = std::io::Cursor::new(Vec::new());
            if raw.write_to(&mut png, image::ImageFormat::Png).is_err() {
                continue;
            }
            let name = illustrations::image_name(images.len() + 1, "png");
            out.push_str(&illustrations::marker_for(&name));
            out.push_str("\n\n");
            images.push(Illustration {
                name,
                bytes: png.into_inner(),
            });
        }
    }
    Ok((out, images))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_pdf_needs_no_library() {
        // `ensure_for` short-circuits on extension before resolving anything,
        // which is what makes importing a .txt work on a broken install too.
        assert_eq!(super::super::parsing::extension_of(Path::new("nota.txt")), "txt");
        assert_eq!(super::super::parsing::extension_of(Path::new("a.PDF")), "pdf");
    }

    /// Reading two PDFs in the same process, which is the whole point: the
    /// failure this guards against (`PdfiumLibraryBindingsAlreadyInitialized`)
    /// only shows up on the SECOND extraction, so a one-shot test proves nothing
    /// about it. Needs the real library and a real PDF, both by environment
    /// variable so the test never guesses a path into the user's own files:
    ///
    /// ```text
    /// READER_PDFIUM_LIBRARY=<repo>/src-tauri/resources/pdfium/bin/pdfium.dll
    /// READER_PDFIUM_TEST_PDF=<some>.pdf
    /// cargo test --lib extracting_two_pdfs -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn extracting_two_pdfs_in_the_same_process_reuses_the_bindings() {
        let library = std::env::var("READER_PDFIUM_LIBRARY")
            .expect("set READER_PDFIUM_LIBRARY to the pdfium shared library");
        let pdf = std::env::var("READER_PDFIUM_TEST_PDF")
            .expect("set READER_PDFIUM_TEST_PDF to a real PDF file");
        remember(Path::new(&library));

        let first = extract_text(Path::new(&pdf)).expect("the first extraction failed");
        let second = extract_text(Path::new(&pdf))
            .expect("the second extraction failed - the bindings were re-initialized");

        assert!(
            !first.trim().is_empty(),
            "the PDF gave no text at all, so this run proves nothing about the second read"
        );
        assert_eq!(first, second, "the same file read twice gave different text");
    }

    /// The illustrations of a real PDF (ILLUS-02). Needs the real library and a
    /// real illustrated PDF, both by environment variable so the test never
    /// guesses a path into the user's own files - the format AD-057
    /// established:
    ///
    /// ```text
    /// READER_PDFIUM_LIBRARY=<repo>/src-tauri/resources/pdfium/bin/pdfium.dll
    /// READER_PDFIUM_TEST_PDF=<illustrated>.pdf
    /// cargo test --lib illustrations_of_a_real_pdf -- --ignored --nocapture
    /// ```
    ///
    /// It prints what it found on purpose: the 64 px floor is a chosen number,
    /// and T7 is where it meets real books. The count and the sizes are what
    /// tell whether it let debris in or dropped a real figure.
    #[test]
    #[ignore]
    fn illustrations_of_a_real_pdf_are_extracted_and_marked_in_the_text() {
        let library = std::env::var("READER_PDFIUM_LIBRARY")
            .expect("set READER_PDFIUM_LIBRARY to the pdfium shared library");
        let pdf = std::env::var("READER_PDFIUM_TEST_PDF")
            .expect("set READER_PDFIUM_TEST_PDF to a real illustrated PDF");
        remember(Path::new(&library));

        let (text, images) = extract_text_and_images(Path::new(&pdf)).expect("extraction failed");

        println!("gravuras: {}", images.len());
        for image in &images {
            println!("  {} - {} bytes", image.name, image.bytes.len());
        }
        assert!(
            !images.is_empty(),
            "nenhuma gravura passou o piso de {} px - ou o PDF nao tem figura, ou o piso esta alto",
            illustrations::MIN_IMAGE_SIDE_PX
        );
        for image in &images {
            assert_eq!(
                &image.bytes[..8],
                b"\x89PNG\r\n\x1a\n",
                "{} nao e um PNG",
                image.name
            );
            assert!(
                text.contains(&illustrations::marker_for(&image.name)),
                "a gravura {} foi extraida mas nao tem marcador no texto",
                image.name
            );
        }
    }

    /// Reading a PDF with no library loaded has to say so, not panic or return
    /// empty text that would be indexed as a valid (blank) document.
    #[test]
    fn extracting_before_the_library_is_resolved_fails_with_a_message() {
        if LIBRARY_PATH.lock().unwrap().is_some() {
            // Another test in this process already resolved it; the assertion
            // below would then be testing nothing.
            return;
        }
        let error = extract_text(Path::new("qualquer.pdf")).unwrap_err();
        assert!(error.contains("leitor de PDF"), "unexpected message: {error}");
    }
}
