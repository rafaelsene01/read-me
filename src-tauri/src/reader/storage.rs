// SPEC: book-reader (READ-18, READ-28, READ-29, READ-31),
//       book-illustrations (ILLUS-03, ILLUS-07, ILLUS-09, ILLUS-10),
//       epub-fidelity (FID-09, FID-10, FID-11)

//! The on-disk layout of a book, and the **only** place in the code that
//! builds a page path.
//!
//! ```text
//! <base_path>/library/
//!     <folder>/             <- books.folder
//!         <filename>        <- the imported file
//!         original/0001.txt <- extracted text, paginated
//!         pt/0001.txt       <- one folder per language
//! ```
//!
//! With the text on disk, "where is page 47 in Portuguese" becomes a question
//! asked from five different places. Five answers drift; one does not. Same
//! reason `split_paragraphs` has a single definition.
//!
//! Everything here is a pure function over `&Path`, so it is exercised against
//! a temp folder with no `AppHandle` — which is what the `book-library`
//! feature could not do, leaving LIB-04/LIB-11 without proof.

use super::illustrations::{is_image_name, Illustration};
use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The folder holding the extracted, untranslated text. It is a language name
/// like any other, so it goes through `lang_dir`: `original` and `pt` differ
/// only in who writes them.
pub const ORIGINAL_DIR: &str = "original";

/// The illustrations of a book, **one folder shared by every language**
/// (ILLUS-03): a picture does not change when the text is translated, so a
/// copy per language would multiply the disk for nothing.
///
/// It is a sibling of the language folders, which is why the two places that
/// sweep subfolders - `wipe_languages` and `book_languages` in
/// `reader_commands` - have to skip it by name. Without that it would show up
/// on screen as a language called "images" and vanish on the first reprocess.
pub const IMAGES_DIR: &str = "images";

/// `<book_dir>/images`.
pub fn images_dir(book_dir: &Path) -> io::Result<PathBuf> {
    child(book_dir, IMAGES_DIR)
}

/// Replaces the whole content of `<book_dir>/images` with `images`.
///
/// Clearing first is the same rule `write_pages` follows: a reprocess with
/// fewer pictures must not leave the previous run's behind, pointed at by
/// nothing (ILLUS-09).
///
/// An empty list leaves **no folder at all**, so a book without illustrations
/// looks on disk exactly as it did before this feature (ILLUS-11).
pub fn write_images(book_dir: &Path, images: &[Illustration]) -> io::Result<()> {
    let dir = images_dir(book_dir)?;
    remove_dir_if_present(&dir)?;
    if images.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir)?;
    for image in images {
        std::fs::write(image_file(&dir, &image.name)?, &image.bytes)?;
    }
    Ok(())
}

/// The bytes of one illustration, by the name a marker carries.
pub fn read_image(book_dir: &Path, name: &str) -> io::Result<Vec<u8>> {
    std::fs::read(image_file(&images_dir(book_dir)?, name)?)
}

/// `<images_dir>/NNNN.<ext>`, and the **only** place a picture name becomes a
/// path.
///
/// The name arrives from the frontend on the read side, so it is checked
/// against the exact shape this crate produces (ILLUS-07). `child` alone would
/// stop `../`, but not a name shaped like nothing this code ever wrote.
fn image_file(images_dir: &Path, name: &str) -> io::Result<PathBuf> {
    if !is_image_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not an illustration name: {name:?}"),
        ));
    }
    child(images_dir, name)
}

/// A folder or language name has to be one ordinary path component.
///
/// This is a guard on a trust boundary, not defensive noise: `books.folder` is
/// nullable until the T17 migration fills it, and the reading language arrives
/// from the frontend. An empty name makes `parent.join(name) == parent`, and
/// both `write_pages` and `remove_lang` call `remove_dir_all` on what they are
/// handed — so an empty language would delete the whole book folder, imported
/// file included, and an empty folder would delete the library.
fn child(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(one)), None) => Ok(parent.join(one)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a single path component: {name:?}"),
        )),
    }
}

/// `<library_dir>/<folder>` — the folder that holds one book (READ-32).
pub fn book_dir(library_dir: &Path, folder: &str) -> io::Result<PathBuf> {
    child(library_dir, folder)
}

/// `<book_dir>/<language>` — one folder per language, side by side (READ-28).
pub fn lang_dir(book_dir: &Path, language: &str) -> io::Result<PathBuf> {
    child(book_dir, language)
}

/// The extensions a page file can have, **most faithful first**.
///
/// `html` is what an EPUB produces now (FID-01); `txt` is what a PDF still
/// produces and what every book processed before this feature has on disk. The
/// order is the fallback rule of FID-09 in one line: a book already processed
/// and **translated** stays readable, and reprocessing promotes it.
pub const PAGE_EXTENSIONS: [&str; 2] = ["html", "txt"];

/// The book's stylesheet, next to `images/` and shared by every language for
/// the same reason (FID-10): translating does not change the typography.
pub const STYLES_DIR: &str = "styles";

/// Neither of these is a language, and the two functions in `reader_commands`
/// that sweep the book's subfolders have to know it by name (FID-11).
pub fn is_reserved_dir(name: &str) -> bool {
    name == IMAGES_DIR || name == STYLES_DIR
}

pub fn styles_file(book_dir: &Path) -> io::Result<PathBuf> {
    Ok(child(book_dir, STYLES_DIR)?.join("book.css"))
}

/// Writes the book's CSS, or removes it when the book has none - so a book
/// with no stylesheet leaves no folder behind, like `write_images`.
pub fn write_css(book_dir: &Path, css: &str) -> io::Result<()> {
    let file = styles_file(book_dir)?;
    if css.trim().is_empty() {
        return remove_dir_if_present(&child(book_dir, STYLES_DIR)?);
    }
    std::fs::create_dir_all(file.parent().expect("styles_file always has a parent"))?;
    std::fs::write(file, css)
}

pub fn read_css(book_dir: &Path) -> String {
    styles_file(book_dir)
        .ok()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .unwrap_or_default()
}

/// `<dir>/NNNN.<ext>`, base 1.
pub fn page_file_ext(dir: &Path, page: u32, extension: &str) -> PathBuf {
    dir.join(format!("{page:04}.{extension}"))
}

/// The page file that exists, and the extension it has. `None` when the page
/// was never written for this language.
pub fn existing_page(dir: &Path, page: u32) -> Option<(PathBuf, &'static str)> {
    PAGE_EXTENSIONS.iter().find_map(|extension| {
        let path = page_file_ext(dir, page, extension);
        path.exists().then_some((path, *extension))
    })
}

/// `<dir>/NNNN.txt`, base 1.
///
/// Zero-padded to four digits so the explorer's alphabetical order *is*
/// reading order: without it `10.txt` sorts before `2.txt` on the user's
/// screen, and the folder is his (READ-31).
pub fn page_file(dir: &Path, page: u32) -> PathBuf {
    dir.join(format!("{page:04}.txt"))
}

/// The same, with the extension chosen by the caller: `html` for an EPUB read
/// faithfully, `txt` for a PDF and for the old format (FID-01, FID-12).
pub fn write_pages_ext(dir: &Path, pages: &[String], extension: &str) -> io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    std::fs::create_dir_all(dir)?;
    for (i, page) in pages.iter().enumerate() {
        std::fs::write(page_file_ext(dir, i as u32 + 1, extension), page.trim_end())?;
    }
    Ok(())
}

/// Replaces the whole content of `dir` with `pages`, page 1 first.
///
/// **It clears the folder first** because a reprocess can be shorter than the
/// last one: 10 pages regenerated as 5 would otherwise leave `0006..0010`
/// behind, and they would read as real pages — the file existing is the only
/// checkpoint there is.
///
/// **Each page is trimmed at the end before it is written.** `paginate` hands
/// over literal slices with the paragraph separator attached to the page that
/// ends, so that `paginate(t).concat() == t`; that trailing blank line is a
/// property of the split, not text the reader wants to see or hand-edit.
/// Trimming once on write is cheaper than trimming on every read, and it makes
/// the file on disk be exactly the page. The accepted cost: reading the pages
/// back and concatenating no longer reproduces the extracted text byte for
/// byte.
pub fn write_pages(dir: &Path, pages: &[String]) -> io::Result<()> {
    write_pages_ext(dir, pages, "txt")
}

/// The page as it is on disk, whichever format it was written in (FID-09).
pub fn read_page(dir: &Path, page: u32) -> io::Result<String> {
    let (path, _) = existing_page(dir, page).ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, format!("no page {page} in {dir:?}"))
    })?;
    std::fs::read_to_string(path)
}

/// The pages present in `dir`, ascending. A folder that does not exist yet is
/// an empty set and not an error: a language nobody translated is zero pages.
///
/// Only names `page_file` itself would produce are counted. A `0003.txt.bak`
/// or a `notas.txt` the user dropped in there is not a page — the folder is
/// his, so it can hold anything.
pub fn translated_pages(dir: &Path) -> BTreeSet<u32> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BTreeSet::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Either format counts as "this page is done" (FID-09): a book
            // half translated before the change must not look half missing.
            PAGE_EXTENSIONS.iter().find_map(|extension| {
                let page: u32 = name.strip_suffix(&format!(".{extension}"))?.parse().ok()?;
                (format!("{page:04}.{extension}") == name).then_some(page)
            })
        })
        .collect()
}

/// The lowest page in `1..=page_count` with no file, or `None` when the
/// language is complete.
///
/// This is the whole resume mechanism: no state column, no queue. It has to
/// find a gap in the middle and not just the first absent page at the end,
/// because a deleted page — deleted by the app to redo it, or by the user in
/// the explorer — is a page that must be made again (READ-31.9).
pub fn next_missing(dir: &Path, page_count: u32) -> Option<u32> {
    let present = translated_pages(dir);
    (1..=page_count).find(|page| !present.contains(page))
}

/// Deletes one language folder and leaves every other one standing (READ-29).
/// Counting what will be lost before asking is the UI's job (T15), and it has
/// `translated_pages` for that.
pub fn remove_lang(book_dir: &Path, language: &str) -> io::Result<()> {
    remove_dir_if_present(&lang_dir(book_dir, language)?)
}

/// Deletes the book folder with every language in it and the imported file
/// (READ-18/HIST-08). There is no page table to cascade from: the code is what
/// deletes, so the proof is about the disk.
pub fn remove_book_dir(library_dir: &Path, folder: &str) -> io::Result<()> {
    remove_dir_if_present(&book_dir(library_dir, folder)?)
}

/// A book the user already deleted by hand must not become impossible to
/// remove — the same rule `remove_book` in `library_commands` follows for the
/// file (LIB-10).
fn remove_dir_if_present(dir: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma pasta de biblioteca vazia por teste, recriada do zero: sobras de
    /// uma execução anterior fariam o teste do "limpa a pasta antes" passar
    /// pelo motivo errado.
    ///
    /// Ela fica sempre sob `std::env::temp_dir()` e **nunca** vem da
    /// configuração do app: nenhum teste pode apontar para a biblioteca real
    /// do usuário, que é o que estas funções apagam.
    fn library(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("readme-storage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn pages(n: u32) -> Vec<String> {
        (1..=n).map(|i| format!("página {i}")).collect()
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn page_files_are_zero_padded_so_the_explorer_sorts_them_in_reading_order() {
        // READ-31. Doze páginas é o menor fixture em que o bug aparece: sem
        // padding, "10.txt" viria antes de "2.txt".
        let lib = library("padding");
        let dir = lang_dir(&book_dir(&lib, "livro").unwrap(), ORIGINAL_DIR).unwrap();
        write_pages(&dir, &pages(12)).unwrap();

        assert_eq!(page_file(&dir, 10).file_name().unwrap(), "0010.txt");
        let sorted = names_in(&dir);
        let reading_order: Vec<String> = (1..=12).map(|i| format!("{i:04}.txt")).collect();
        assert_eq!(sorted, reading_order);
        assert_eq!(
            sorted.iter().position(|n| n == "0010.txt"),
            Some(9),
            "0010.txt não ficou depois de 0002.txt na ordem que o usuário vê"
        );
        assert_eq!(read_page(&dir, 10).unwrap(), "página 10");
    }

    #[test]
    fn next_missing_finds_the_first_gap_not_the_first_absent_at_the_end() {
        let lib = library("gap");
        let dir = lang_dir(&book_dir(&lib, "livro").unwrap(), "pt").unwrap();
        write_pages(&dir, &pages(10)).unwrap();
        std::fs::remove_file(page_file(&dir, 4)).unwrap();
        std::fs::remove_file(page_file(&dir, 9)).unwrap();

        assert_eq!(next_missing(&dir, 10), Some(4));
        assert_eq!(translated_pages(&dir).len(), 8);
    }

    #[test]
    fn next_missing_returns_none_when_the_language_is_complete() {
        let lib = library("complete");
        let dir = lang_dir(&book_dir(&lib, "livro").unwrap(), "pt").unwrap();
        write_pages(&dir, &pages(5)).unwrap();

        assert_eq!(next_missing(&dir, 5), None);
        // Um idioma que ninguém começou é a página 1, não um erro de leitura.
        let untouched = lang_dir(&book_dir(&lib, "livro").unwrap(), "en").unwrap();
        assert_eq!(next_missing(&untouched, 5), Some(1));
        assert!(translated_pages(&untouched).is_empty());
    }

    #[test]
    fn a_file_deleted_by_hand_makes_that_page_pending_again() {
        // READ-31.9: o usuário apaga pt/0003.txt pelo explorador e o app não
        // tem como saber que foi ele — o arquivo ausente É a marca.
        let lib = library("deleted-by-hand");
        let book = book_dir(&lib, "livro").unwrap();
        let pt = lang_dir(&book, "pt").unwrap();
        write_pages(&pt, &pages(6)).unwrap();
        assert_eq!(next_missing(&pt, 6), None);

        std::fs::remove_file(pt.join("0003.txt")).unwrap();

        assert_eq!(next_missing(&pt, 6), Some(3));
        assert!(read_page(&pt, 3).is_err());
        // Só aquela página: as vizinhas não são refeitas.
        assert_eq!(read_page(&pt, 2).unwrap(), "página 2");
        assert_eq!(read_page(&pt, 4).unwrap(), "página 4");
    }

    #[test]
    fn two_language_folders_coexist_in_the_same_book_folder() {
        // READ-28. O mesmo índice de página nos dois, que é propriedade de
        // projeto e não coincidência: a paginação roda sobre o original e a
        // tradução é por página.
        let lib = library("two-languages");
        let book = book_dir(&lib, "livro").unwrap();
        write_pages(&lang_dir(&book, ORIGINAL_DIR).unwrap(), &pages(10)).unwrap();
        write_pages(
            &lang_dir(&book, "pt").unwrap(),
            &(1..=10).map(|i| format!("pt {i}")).collect::<Vec<_>>(),
        )
        .unwrap();
        write_pages(
            &lang_dir(&book, "en").unwrap(),
            &(1..=10).map(|i| format!("en {i}")).collect::<Vec<_>>(),
        )
        .unwrap();

        assert_eq!(names_in(&book), vec!["en", "original", "pt"]);
        assert_eq!(read_page(&lang_dir(&book, "pt").unwrap(), 6).unwrap(), "pt 6");
        assert_eq!(read_page(&lang_dir(&book, "en").unwrap(), 6).unwrap(), "en 6");
        assert_eq!(translated_pages(&lang_dir(&book, "en").unwrap()).len(), 10);
    }

    #[test]
    fn remove_lang_deletes_one_folder_and_leaves_the_others() {
        // READ-29, só a metade do disco. "Informar quantas páginas serão
        // perdidas antes de aplicar" é UI (T15) e NÃO é provado aqui; o que
        // este teste prova é que a contagem está disponível antes de apagar.
        let lib = library("remove-lang");
        let book = book_dir(&lib, "livro").unwrap();
        std::fs::create_dir_all(&book).unwrap();
        std::fs::write(book.join("livro.epub"), b"bytes").unwrap();
        write_pages(&lang_dir(&book, ORIGINAL_DIR).unwrap(), &pages(4)).unwrap();
        write_pages(&lang_dir(&book, "pt").unwrap(), &pages(4)).unwrap();
        write_pages(&lang_dir(&book, "en").unwrap(), &pages(4)).unwrap();

        assert_eq!(translated_pages(&lang_dir(&book, "pt").unwrap()).len(), 4);
        remove_lang(&book, "pt").unwrap();

        assert_eq!(names_in(&book), vec!["en", "livro.epub", "original"]);
        assert_eq!(translated_pages(&lang_dir(&book, "en").unwrap()).len(), 4);
        // Apagar duas vezes não é erro: a pasta é do usuário e ele pode
        // tê-la apagado antes.
        remove_lang(&book, "pt").unwrap();
    }

    #[test]
    fn remove_book_dir_takes_every_language_with_it() {
        // READ-18/HIST-08, só a metade do disco. Apagar a linha de `books` é
        // `library_commands` (T7) e NÃO é exercitado por este teste.
        let lib = library("remove-book");
        let book = book_dir(&lib, "livro").unwrap();
        std::fs::create_dir_all(&book).unwrap();
        std::fs::write(book.join("livro.epub"), b"bytes").unwrap();
        for lang in [ORIGINAL_DIR, "pt", "en"] {
            write_pages(&lang_dir(&book, lang).unwrap(), &pages(3)).unwrap();
        }
        let other = book_dir(&lib, "outro").unwrap();
        write_pages(&lang_dir(&other, "pt").unwrap(), &pages(3)).unwrap();

        remove_book_dir(&lib, "livro").unwrap();

        assert!(!book.exists(), "a pasta do livro sobreviveu");
        assert_eq!(names_in(&lib), vec!["outro"], "levou o livro vizinho junto");
        assert_eq!(translated_pages(&lang_dir(&other, "pt").unwrap()).len(), 3);
        remove_book_dir(&lib, "livro").unwrap();
    }

    #[test]
    fn write_pages_clears_the_folder_first_so_a_shorter_reprocess_leaves_no_leftovers() {
        let lib = library("shorter");
        let dir = lang_dir(&book_dir(&lib, "livro").unwrap(), ORIGINAL_DIR).unwrap();
        write_pages(&dir, &pages(10)).unwrap();
        assert_eq!(translated_pages(&dir).len(), 10);

        write_pages(&dir, &pages(5)).unwrap();

        assert_eq!(
            translated_pages(&dir),
            (1..=5).collect::<BTreeSet<u32>>(),
            "0006..0010 ficaram órfãs e seriam lidas como páginas de verdade"
        );
        assert!(!page_file(&dir, 6).exists());
        assert_eq!(next_missing(&dir, 5), None);
    }

    #[test]
    fn write_pages_trims_the_page_separator_paginate_leaves_at_the_end() {
        // A decisão que a T4 deixou em aberto, tomada aqui: `paginate` devolve
        // fatias literais com a linha em branco presa à página que termina,
        // para que `paginate(t).concat() == t`. Esse separador é propriedade
        // da divisão, não texto para exibir ou editar à mão. Aparado uma vez
        // na escrita em vez de a cada leitura — o par que a T5 e a T9 usam.
        let lib = library("trim");
        let dir = lang_dir(&book_dir(&lib, "livro").unwrap(), ORIGINAL_DIR).unwrap();
        let raw = vec!["Primeira página.\n\n".to_string(), "Segunda.".to_string()];

        write_pages(&dir, &raw).unwrap();

        assert_eq!(read_page(&dir, 1).unwrap(), "Primeira página.");
        assert_eq!(read_page(&dir, 2).unwrap(), "Segunda.");
        // O custo, aceito e dito: reler as páginas não remonta mais o texto
        // extraído byte a byte.
        assert_ne!(
            format!(
                "{}{}",
                read_page(&dir, 1).unwrap(),
                read_page(&dir, 2).unwrap()
            ),
            raw.concat()
        );
    }

    fn illustration(name: &str, bytes: &[u8]) -> Illustration {
        Illustration {
            name: name.to_string(),
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn an_html_page_is_preferred_over_a_txt_one_and_both_count_as_done() {
        // FID-09. Um livro já processado no formato antigo continua legível, e
        // reprocessar promove: os dois arquivos podem coexistir por um
        // instante, e o mais fiel é o que ganha.
        let lib = library("page-formats");
        let dir = lang_dir(&book_dir(&lib, "livro").unwrap(), ORIGINAL_DIR).unwrap();
        write_pages_ext(&dir, &["<p>novo</p>".to_string()], "html").unwrap();
        std::fs::write(page_file_ext(&dir, 1, "txt"), "antigo").unwrap();

        assert_eq!(read_page(&dir, 1).unwrap(), "<p>novo</p>");
        assert_eq!(existing_page(&dir, 1).unwrap().1, "html");
        assert_eq!(translated_pages(&dir), (1..=1).collect::<BTreeSet<u32>>());

        // Só o `.txt`: continua sendo uma página completa, não uma lacuna.
        let old = lang_dir(&book_dir(&lib, "livro").unwrap(), "pt").unwrap();
        write_pages(&old, &["antigo".to_string()]).unwrap();
        assert_eq!(read_page(&old, 1).unwrap(), "antigo");
        assert_eq!(existing_page(&old, 1).unwrap().1, "txt");
        assert_eq!(next_missing(&old, 1), None);
        assert!(read_page(&old, 2).is_err());
    }

    #[test]
    fn the_stylesheet_lives_beside_the_images_and_neither_is_a_language() {
        // FID-10/FID-11.
        let lib = library("styles");
        let book = book_dir(&lib, "livro").unwrap();
        write_pages_ext(&lang_dir(&book, ORIGINAL_DIR).unwrap(), &["<p>a</p>".to_string()], "html")
            .unwrap();
        write_images(&book, &[illustration("0001.png", b"um")]).unwrap();
        write_css(&book, "p { text-align: justify; }").unwrap();

        assert_eq!(names_in(&book), vec!["images", "original", "styles"]);
        assert_eq!(read_css(&book), "p { text-align: justify; }");
        assert!(is_reserved_dir("images") && is_reserved_dir("styles"));
        assert!(!is_reserved_dir("pt") && !is_reserved_dir(ORIGINAL_DIR));

        // Livro sem CSS não deixa pasta, como acontece com as imagens.
        write_css(&book, "   ").unwrap();
        assert_eq!(names_in(&book), vec!["images", "original"]);
        assert_eq!(read_css(&book), "");
    }

    #[test]
    fn one_images_folder_serves_every_language_and_a_reprocess_leaves_no_orphan() {
        // ILLUS-03 e ILLUS-09. As traduções não copiam gravura nenhuma: elas
        // apontam para os mesmos arquivos, que é o que esta pasta única
        // significa em disco.
        let lib = library("images");
        let book = book_dir(&lib, "livro").unwrap();
        write_pages(&lang_dir(&book, ORIGINAL_DIR).unwrap(), &pages(2)).unwrap();
        write_pages(&lang_dir(&book, "pt").unwrap(), &pages(2)).unwrap();
        write_images(
            &book,
            &[
                illustration("0001.png", b"um"),
                illustration("0002.png", b"dois"),
                illustration("0003.png", b"tres"),
            ],
        )
        .unwrap();

        assert_eq!(names_in(&book), vec!["images", "original", "pt"]);
        assert_eq!(read_image(&book, "0002.png").unwrap(), b"dois");

        // Reprocessar com menos gravuras: a terceira não pode sobrar.
        write_images(&book, &[illustration("0001.png", b"novo")]).unwrap();

        assert_eq!(names_in(&images_dir(&book).unwrap()), vec!["0001.png"]);
        assert_eq!(read_image(&book, "0001.png").unwrap(), b"novo");
        assert!(read_image(&book, "0003.png").is_err());

        // Um livro sem gravura nenhuma não deixa pasta (ILLUS-11).
        write_images(&book, &[]).unwrap();
        assert!(!images_dir(&book).unwrap().exists());
        assert_eq!(names_in(&book), vec!["original", "pt"]);
    }

    #[test]
    fn removing_the_book_takes_the_images_with_it() {
        // ILLUS-09, segunda metade.
        let lib = library("images-removed");
        let book = book_dir(&lib, "livro").unwrap();
        write_pages(&lang_dir(&book, ORIGINAL_DIR).unwrap(), &pages(1)).unwrap();
        write_images(&book, &[illustration("0001.png", b"um")]).unwrap();

        remove_book_dir(&lib, "livro").unwrap();

        assert!(!images_dir(&book).unwrap().exists());
        assert!(!book.exists());
    }

    #[test]
    fn an_illustration_name_that_is_not_the_exact_shape_is_refused() {
        // ILLUS-07. Este nome é o único desta feature que volta do frontend.
        let lib = library("images-guard");
        let book = book_dir(&lib, "livro").unwrap();
        write_images(&book, &[illustration("0001.png", b"um")]).unwrap();
        std::fs::write(book.join("segredo.txt"), b"nao ler").unwrap();

        for bad in [
            "../segredo.txt",
            "..",
            "",
            "0001.png/../../segredo.txt",
            "segredo.txt",
            "0001.PNG",
            "1.png",
        ] {
            assert!(read_image(&book, bad).is_err(), "leu {bad:?}");
        }
        assert_eq!(read_image(&book, "0001.png").unwrap(), b"um");
    }

    #[test]
    fn an_empty_or_escaping_name_is_refused_instead_of_deleting_the_parent_folder() {
        // A linha mais perigosa desta feature é `remove_dir_all`. Um idioma
        // vazio faria `lang_dir` devolver a própria pasta do livro, e
        // `remove_lang` levaria o arquivo importado junto; uma pasta vazia
        // miraria `library/`. Os dois são alcançáveis: `books.folder` é NULL
        // até a migração da T17 rodar, e o idioma vem da UI.
        let lib = library("guard");
        let book = book_dir(&lib, "livro").unwrap();
        write_pages(&lang_dir(&book, "pt").unwrap(), &pages(2)).unwrap();

        for bad in ["", ".", "..", "pt/0001.txt", "../outro"] {
            assert!(lang_dir(&book, bad).is_err(), "aceitou {bad:?}");
            assert!(book_dir(&lib, bad).is_err(), "aceitou {bad:?}");
            assert!(remove_lang(&book, bad).is_err(), "apagaria com {bad:?}");
            assert!(remove_book_dir(&lib, bad).is_err(), "apagaria com {bad:?}");
        }
        assert!(book.exists());
        assert_eq!(translated_pages(&lang_dir(&book, "pt").unwrap()).len(), 2);
    }
}
