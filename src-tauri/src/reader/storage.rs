// SPEC: book-reader (READ-18, READ-28, READ-29, READ-31)

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

use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The folder holding the extracted, untranslated text. It is a language name
/// like any other, so it goes through `lang_dir`: `original` and `pt` differ
/// only in who writes them.
pub const ORIGINAL_DIR: &str = "original";

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

/// `<dir>/NNNN.txt`, base 1.
///
/// Zero-padded to four digits so the explorer's alphabetical order *is*
/// reading order: without it `10.txt` sorts before `2.txt` on the user's
/// screen, and the folder is his (READ-31).
pub fn page_file(dir: &Path, page: u32) -> PathBuf {
    dir.join(format!("{page:04}.txt"))
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
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        // Not an error: the first processing has no folder yet. Any other
        // failure has to surface, or stale pages would survive the clear.
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    std::fs::create_dir_all(dir)?;
    for (i, page) in pages.iter().enumerate() {
        std::fs::write(page_file(dir, i as u32 + 1), page.trim_end())?;
    }
    Ok(())
}

pub fn read_page(dir: &Path, page: u32) -> io::Result<String> {
    std::fs::read_to_string(page_file(dir, page))
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
            let page: u32 = name.strip_suffix(".txt")?.parse().ok()?;
            (format!("{page:04}.txt") == name).then_some(page)
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
