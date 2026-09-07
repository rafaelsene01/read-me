// SPEC: book-reader (READ-10), book-illustrations (ILLUS-12)

//! Turning an extracted book into pages, deterministically.
//!
//! The one invariant everything else leans on: `paginate(t).concat() == t`.
//! Pages are literal slices of the input, separator included, so no character
//! is invented and none is lost. That is why a page can end with the blank
//! line that separated it from the next paragraph — the display side trims,
//! the storage side does not have to guess what was dropped.

use super::illustrations::{IMAGE_BUDGET_CHARS, MARKER_PREFIX};

/// Characters per page. Measured in T1: the real page used to time the
/// translation had 2.161 characters in 3 paragraphs, and ~2.500 is the
/// comfortable-to-read size that number sits inside. Tuning it is T13's job,
/// looking at the screen — it is a number, not a refactor.
pub const PAGE_BUDGET_CHARS: usize = 2_500;

/// The paragraphs of a text, trimmed, in order. **One definition of
/// "paragraph" for the whole app**: `paginate` uses it to find its page
/// boundaries and the translation loop (T6) uses it to size one request. Two
/// definitions would drift, and the one that drifted would cut a paragraph in
/// half — exactly what READ-22 exists to prevent.
pub fn split_paragraphs(text: &str) -> Vec<&str> {
    let mut bounds = paragraph_starts(text);
    bounds.push(text.len());

    let mut out = Vec::new();
    let mut start = 0;
    for end in bounds {
        let paragraph = text[start..end].trim();
        if !paragraph.is_empty() {
            out.push(paragraph);
        }
        start = end;
    }
    out
}

/// Splits `text` into pages of at most [`PAGE_BUDGET_CHARS`] characters,
/// preferring the last paragraph boundary that fits, then the last sentence
/// boundary, then the budget itself (READ-10).
///
/// The three-level fallback is what makes "a page is a whole number of
/// paragraphs" true rather than lucky: a paragraph that fits is never cut,
/// because its own end boundary is always inside the budget and the search
/// takes the largest one.
pub fn paginate(text: &str) -> Vec<String> {
    // Whitespace-only text never reaches here: extraction rejects it first
    // (`ParseError::NoTextFound`, rag/parsing.rs). Returning no pages keeps a
    // failed import from producing a blank page 0.
    if text.trim().is_empty() {
        return Vec::new();
    }

    let paragraphs = paragraph_starts(text);
    let sentences = sentence_starts(text);

    let mut pages = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let limit = budget_end(text, start);
        let end = if limit == text.len() {
            limit
        } else {
            last_boundary_in(&paragraphs, start, limit)
                .or_else(|| last_boundary_in(&sentences, start, limit))
                .unwrap_or(limit)
        };
        pages.push(text[start..end].to_string());
        start = end;
    }
    pages
}

/// Byte offset where the page budget runs out after `start`, or the end of the
/// text. Walking `char_indices` means the cut is on a character boundary by
/// construction, and that the budget counts characters and not bytes — "ação"
/// is 4 characters of page, not 6.
///
/// An image marker is charged [`IMAGE_BUDGET_CHARS`] on top of its own
/// characters (ILLUS-12): its 19 characters buy a picture, and a page that
/// counted only the text would stack five illustrations believing it had spent
/// nothing. Only the prefix is looked for here, not the whole marker — this
/// runs once per character, and over-charging a paragraph of prose that quotes
/// `[[image: ` costs one page break, while parsing at every offset costs the
/// pagination of every book.
fn budget_end(text: &str, start: usize) -> usize {
    let mut spent = 0;
    for (i, _) in text[start..].char_indices() {
        if spent >= PAGE_BUDGET_CHARS {
            return start + i;
        }
        if text[start + i..].starts_with(MARKER_PREFIX) {
            spent += IMAGE_BUDGET_CHARS;
        }
        spent += 1;
    }
    text.len()
}

/// The largest boundary in `start < b <= limit`. The list is built in
/// ascending order, so the scan is a partition point.
fn last_boundary_in(boundaries: &[usize], start: usize, limit: usize) -> Option<usize> {
    let upto = boundaries.partition_point(|&b| b <= limit);
    boundaries[..upto].last().copied().filter(|&b| b > start)
}

/// Offsets where a paragraph *begins*, excluding offset 0.
///
/// A separator is a whitespace run holding two or more newlines — not the
/// literal `"\n\n"`. `xhtml_to_text` emits exactly `"\n\n"`, but pdfium output
/// carries indentation, so `"\n  \n"` is the same break to a reader and would
/// be invisible to a literal match.
///
/// The offset lands *after* the whitespace, so the blank line stays attached
/// to the page that ends, and a page never opens on empty lines.
fn paragraph_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // Multi-byte UTF-8 has the high bit set, so an ASCII whitespace test
        // on raw bytes can never land inside a character.
        if !bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let run_start = i;
        let mut newlines = 0;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            if bytes[i] == b'\n' {
                newlines += 1;
            }
            i += 1;
        }
        if newlines >= 2 && run_start > 0 && i < bytes.len() {
            starts.push(i);
        }
    }
    starts
}

/// Offsets where a sentence *begins*, used only when a single paragraph is
/// wider than a page.
///
/// A terminator has to be followed by whitespace to count, which is what keeps
/// "3.14" and "art. 5" from being read as two sentences. Closing quotes and
/// brackets belong to the sentence that ends, so `disse "sim."` breaks after
/// the quote and not before it.
fn sentence_starts(text: &str) -> Vec<usize> {
    const PENDING_NONE: u8 = 0;
    const PENDING_TERMINATOR: u8 = 1;
    const PENDING_SPACE: u8 = 2;

    let mut starts = Vec::new();
    let mut state = PENDING_NONE;
    for (i, c) in text.char_indices() {
        state = match (state, c) {
            (_, '.' | '!' | '?' | '…') => PENDING_TERMINATOR,
            (PENDING_TERMINATOR, '"' | '\'' | ')' | ']' | '»' | '\u{201d}' | '\u{2019}') => {
                PENDING_TERMINATOR
            }
            (PENDING_TERMINATOR | PENDING_SPACE, c) if c.is_whitespace() => PENDING_SPACE,
            (PENDING_SPACE, _) => {
                starts.push(i);
                PENDING_NONE
            }
            _ => PENDING_NONE,
        };
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um parágrafo de aproximadamente `chars` caracteres, terminado em ponto.
    /// O acento em "Parágrafo" é de propósito: ele faz `chars != bytes`, que é
    /// o que exercita o orçamento contado em caracteres.
    fn paragraph(tag: usize, chars: usize) -> String {
        let mut p = format!("Parágrafo {tag}");
        while p.chars().count() + 1 < chars {
            p.push_str(" palavra");
        }
        p.push('.');
        p
    }

    /// `n` parágrafos separados por linha em branco — a saída que
    /// `reader::epub::extract_epub_text` produz.
    fn book(n: usize, chars_each: usize) -> String {
        (0..n)
            .map(|i| paragraph(i, chars_each))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[test]
    fn the_same_text_paginates_the_same_way_twice() {
        // ⚠️ INCONCLUSIVO QUANTO A DETERMINISMO ENTRE EXECUÇÕES. Duas chamadas
        // dentro do mesmo processo sempre concordam se a função não tiver
        // estado — este teste não pode provar o contrário. O que ele trava é
        // exatamente isso: que `paginate` continue sem estado interno e sem
        // nenhuma ordem vinda de HashMap/HashSet, cuja iteração o Rust
        // aleatoriza por processo. Se alguém introduzir um desses, as duas
        // Strings iguais mas construídas separadamente é que vão divergir.
        let a = book(20, 400);
        let b = book(20, 400);
        assert_eq!(a, b, "o fixture não é o mesmo texto duas vezes");

        let first = paginate(&a);
        let second = paginate(&b);

        assert!(
            first.len() > 1,
            "fixture pequeno demais: uma página só não prova fronteira nenhuma"
        );
        assert_eq!(first, second);
    }

    #[test]
    fn pages_break_on_paragraph_boundaries() {
        let text = book(20, 400);
        let starts = paragraph_starts(&text);
        assert_eq!(starts.len(), 19, "o fixture deveria ter 20 parágrafos");

        let pages = paginate(&text);
        assert!(pages.len() > 1);

        let mut offset = 0;
        for page in &pages {
            assert!(
                page.chars().count() <= PAGE_BUDGET_CHARS,
                "página com {} caracteres passou do teto de {PAGE_BUDGET_CHARS}",
                page.chars().count()
            );
            offset += page.len();
            if offset < text.len() {
                assert!(
                    starts.contains(&offset),
                    "a página terminou no byte {offset}, que não é começo de parágrafo"
                );
            }
        }
        assert_eq!(offset, text.len());
        assert_eq!(pages.concat(), text);
    }

    #[test]
    fn a_paragraph_larger_than_a_page_falls_back_to_sentence_boundaries() {
        // Um parágrafo só, sem linha em branco nenhuma, com frases de ~120
        // caracteres: a fronteira de parágrafo não existe e a paginação tem
        // de descer um degrau.
        let text = (0..60)
            .map(|i| paragraph(i, 120))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            paragraph_starts(&text).is_empty(),
            "o fixture deixou de ser um parágrafo só"
        );
        assert!(text.chars().count() > 2 * PAGE_BUDGET_CHARS);

        let pages = paginate(&text);

        assert!(pages.len() >= 3);
        assert_eq!(pages.concat(), text);
        for page in &pages[..pages.len() - 1] {
            assert!(
                page.trim_end().ends_with('.'),
                "uma página foi cortada no meio de uma frase: termina em {:?}",
                page.trim_end().chars().rev().take(20).collect::<String>()
            );
        }
    }

    #[test]
    fn a_sentence_larger_than_a_page_is_cut_at_the_budget_and_loses_nothing() {
        // Nem ponto final, nem linha em branco: não existe fronteira nenhuma
        // para escolher, e o único corte possível é o teto.
        let text = (0..1_500)
            .map(|i| format!("palavra{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(paragraph_starts(&text).is_empty());
        assert!(sentence_starts(&text).is_empty());
        assert!(text.chars().count() > 3 * PAGE_BUDGET_CHARS);

        let pages = paginate(&text);

        assert!(pages.len() >= 4);
        for page in &pages[..pages.len() - 1] {
            assert_eq!(page.chars().count(), PAGE_BUDGET_CHARS);
        }
        assert_eq!(
            pages.concat(),
            text,
            "a concatenação das páginas não devolveu o texto de entrada"
        );
    }

    #[test]
    fn a_page_does_not_stack_illustrations_because_the_marker_costs_what_it_shows() {
        // ILLUS-12. Dez marcadores e nada mais: sem cobrança, os 190
        // caracteres caberiam todos numa página só e o leitor receberia dez
        // gravuras empilhadas.
        let markers: Vec<String> = (1..=10)
            .map(|i| super::super::illustrations::marker_for(&format!("{i:04}.png")))
            .collect();
        let text = markers.join("

");
        assert!(text.chars().count() < PAGE_BUDGET_CHARS, "o fixture cabe numa página por tamanho");

        let pages = paginate(&text);

        assert!(
            pages.len() > 1,
            "as {} gravuras foram para uma página só", markers.len()
        );
        let per_page = PAGE_BUDGET_CHARS / IMAGE_BUDGET_CHARS;
        for page in &pages {
            assert!(
                split_paragraphs(page).len() <= per_page + 1,
                "uma página juntou gravuras demais: {:?}",
                split_paragraphs(page)
            );
        }
        // A invariante da READ-10 continua valendo com marcador no meio.
        assert_eq!(pages.concat(), text);
    }

    #[test]
    fn an_empty_text_produces_no_pages() {
        assert!(paginate("").is_empty());
        assert!(split_paragraphs("").is_empty());
        // Texto só de espaço em branco não chega aqui na prática: a extração o
        // recusa antes, como `ParseError::NoTextFound`. Ele é a ÚNICA entrada
        // em que a concatenação das páginas não devolve o original, e isso é
        // deliberado — um livro vazio não pode virar uma página em branco.
        assert!(paginate("   \n\n  \t ").is_empty());
    }

    #[test]
    fn split_paragraphs_agrees_with_the_page_boundaries_paginate_chose() {
        // A prova de que as duas decisões compõem (READ-10 sustentando a
        // fronteira de que a READ-22 depende): com todo parágrafo abaixo do
        // teto, os parágrafos vistos página a página são exatamente os
        // parágrafos do livro, na mesma ordem e sem nenhum partido em dois.
        let text = book(30, 300);
        let pages = paginate(&text);
        assert!(pages.len() > 1);

        let whole = split_paragraphs(&text);
        let by_page: Vec<&str> = pages.iter().flat_map(|p| split_paragraphs(p)).collect();

        assert_eq!(whole.len(), 30);
        assert_eq!(
            whole, by_page,
            "uma página cortou um parágrafo que caberia inteiro"
        );
    }
}
