// SPEC: read-aloud (TTS-02, TTS-03, TTS-08)

//! Cutting a page into what gets spoken, and saying when each piece plays.
//!
//! The unit here is the **sentence**, and that is a decision with a reason: the
//! duration of a sentence is *measured* — it is the size of the WAV the
//! synthesizer produced — while the position of a word inside it would be
//! interpolated. A review of this feature called that fake precision, with the
//! argument that settled it: "1999" is four characters and five syllables, and
//! CJK has no spaces to weigh at all. So the mark moves sentence by sentence,
//! on numbers that were measured, and word-level waits for the drift to be
//! measured against a real book.
//!
//! Everything in this file is a pure function over `&str`, which is what lets
//! it be the one part of read-aloud with real test coverage — the audio, the
//! child process and the marking on screen are all UAT.
//!
//! **There is no duration arithmetic here, and that is deliberate.** An earlier
//! version computed the sentence length from the WAV header — correct, and
//! redundant: the `<audio>` element that plays the file already knows its own
//! length and fires `ended`. Two clocks can disagree; one cannot. The mark moves
//! on the element's event, so the spike's measurement (91.296 bytes at 22.050 Hz
//! = 2,069 s) stayed a fact about the format and never became code to maintain.

use crate::reader::epub;
use crate::reader::html;
use crate::reader::illustrations;
use crate::reader::pagination;

/// One spoken unit: the text handed to the synthesizer, and where it sits in
/// the page so the screen can mark it.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    /// Index of the block this sentence came from, in `split_blocks` order.
    pub block: usize,
    /// Index of the sentence inside that block, base 0.
    pub sentence: usize,
    /// What the synthesizer is asked to say. Never markup.
    pub text: String,
}

/// Every sentence of a page, in reading order (TTS-02).
///
/// Blocks with no visible text — a figure, a rule — produce nothing, which is
/// what makes TTS-08 true by construction: they cannot cost a request because
/// they never become an utterance.
pub fn utterances(page_html: &str) -> Vec<Utterance> {
    // The screen hands over the **assembled document** - doctype, head, the
    // book's stylesheet, body - because that is what it renders. Reading starts
    // by keeping only the body: everything above it is metadata and CSS, and a
    // voice that reads those is reading the page's plumbing out loud.
    let body = epub::body_of(page_html);
    let mut out = Vec::new();
    for (block, markup) in html::split_blocks(body).iter().enumerate() {
        // Markers come out **before** the text is cut into sentences: on a
        // `.txt` page the whole page is one block, so a marker with no full
        // stop after it belongs to the sentence beside it, and dropping whole
        // sentences would take the prose with it.
        let text = illustrations::without_markers(&html::visible_text(markup));
        if !is_speakable(&text) {
            continue;
        }
        let mut sentence = 0;
        for piece in split_sentences(&text) {
            if !is_speakable(&piece) {
                continue;
            }
            out.push(Utterance {
                block,
                sentence,
                text: piece,
            });
            sentence += 1;
        }
    }
    out
}

/// Whether a piece of text is words, rather than something a voice would spell
/// out loud.
///
/// Two things are refused, and both were reaching the synthesizer:
///
/// 1. **A line that was only an illustration marker.** The markers themselves
///    are already gone by here — `illustrations::without_markers` runs on the
///    block, because a marker glued to a sentence is not a paragraph and would
///    survive a paragraph-shaped check. What this catches is the leftover: a
///    block that was nothing but pictures.
/// 2. **A line with no letters and no digits at all.** Scene breaks (`* * *`),
///    rules (`———`) and stray bullets carry meaning to the eye and nothing to
///    the ear; spoken, they become noise between two paragraphs.
///
/// Punctuation **inside** a sentence stays untouched, and that is the line this
/// function is careful not to cross: a comma and a full stop are what tell the
/// synthesizer where to breathe. Stripping them would not make the reading
/// cleaner, it would make it monotone.
fn is_speakable(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || illustrations::marker_name(text).is_some() {
        return false;
    }
    text.chars().any(|c| c.is_alphanumeric())
}

/// The sentences of a run of plain text.
///
/// It leans on `pagination::sentence_starts`, which is **the** definition of a
/// sentence boundary in this app. A second definition would drift, and the one
/// that drifted would cut a sentence in the middle of the audio — the same
/// argument that keeps `split_paragraphs` single (READ-22).
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut bounds = pagination::sentence_starts(text);
    bounds.push(text.len());
    let mut out = Vec::new();
    let mut start = 0;
    for end in bounds {
        if end <= start {
            continue;
        }
        let piece = text[start..end].trim();
        if !piece.is_empty() {
            out.push(piece.to_string());
        }
        start = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_becomes_sentences_in_reading_order() {
        let page = concat!(
            "<p>Primeira frase. Segunda frase!</p>\n\n",
            "<div class=\"fig\"><img src=\"0001.png\"/></div>\n\n",
            "<p>Terceira.</p>"
        );

        let out = utterances(page);

        assert_eq!(
            out,
            vec![
                Utterance { block: 0, sentence: 0, text: "Primeira frase.".into() },
                Utterance { block: 0, sentence: 1, text: "Segunda frase!".into() },
                Utterance { block: 2, sentence: 0, text: "Terceira.".into() },
            ]
        );
    }

    #[test]
    fn the_whole_assembled_document_reads_as_the_book_and_nothing_else() {
        // Este é o formato exato que o `ReaderPanel` recebe e passa adiante:
        // documento montado, com o CSS do livro dentro. Antes desta correção a
        // primeira "frase" era a folha de estilo.
        let document = concat!(
            "<!doctype html><html><head><meta charset=\"utf-8\">",
            "<style>body{margin:0 auto;font-family:Georgia,serif}</style>",
            "<style>p.versalete{font-variant:small-caps}</style></head>",
            "<body><p>Prezado Caos,</p>",
            "<p>Pelo menos é assim que te chamam.</p></body></html>"
        );

        let out = utterances(document);

        assert_eq!(
            out.iter().map(|u| u.text.as_str()).collect::<Vec<_>>(),
            vec!["Prezado Caos,", "Pelo menos é assim que te chamam."]
        );
        for u in &out {
            assert!(!u.text.contains('{'), "levou CSS para a voz: {:?}", u.text);
            assert!(!u.text.contains("font-family"), "levou CSS: {:?}", u.text);
        }
    }

    #[test]
    fn what_is_read_is_words_and_never_the_name_of_a_picture() {
        // Numa página `.txt` — um PDF, ou um livro do formato antigo — o
        // marcador é texto comum, e a voz leria o nome do arquivo letra por
        // letra. É a única coisa da página que não é prosa do livro.
        let page = concat!(
            "Primeiro parágrafo.\n\n",
            "[[image: 0001.png]]\n\n",
            "Segundo parágrafo."
        );

        let out = utterances(page);

        assert_eq!(
            out.iter().map(|u| u.text.as_str()).collect::<Vec<_>>(),
            vec!["Primeiro parágrafo.", "Segundo parágrafo."]
        );
        assert!(!out.iter().any(|u| u.text.contains("[[image")));
    }

    #[test]
    fn a_decoration_with_no_letters_is_not_spoken() {
        // Quebra de cena e filete dizem algo ao olho e nada ao ouvido: falados,
        // viram ruído entre dois parágrafos.
        for decoration in ["* * *", "———", "• • •", "···", "§"] {
            assert!(
                utterances(&format!("<p>{decoration}</p>")).is_empty(),
                "leu uma decoração em voz alta: {decoration:?}"
            );
        }
    }

    #[test]
    fn punctuation_inside_a_sentence_is_kept_because_it_is_the_prosody() {
        // O limite que esta regra não pode cruzar: vírgula e ponto são o que
        // dizem ao sintetizador onde respirar. Tirá-los deixaria a leitura
        // monótona, não mais limpa.
        let out = utterances("<p>Sim, claro — era isso mesmo! E agora?</p>");
        assert_eq!(
            out.iter().map(|u| u.text.as_str()).collect::<Vec<_>>(),
            vec!["Sim, claro — era isso mesmo!", "E agora?"]
        );
    }

    #[test]
    fn the_sentence_index_stays_contiguous_after_something_is_dropped() {
        // O índice é o que o frontend usa para achar a frase na página: um
        // buraco nele deslocaria a marcação de tudo que vem depois.
        let out = utterances("<p>Uma. * * * Duas.</p>");
        assert_eq!(
            out.iter().map(|u| u.sentence).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn a_block_with_no_visible_text_never_becomes_an_utterance() {
        // TTS-08 é verdade por construção: a figura não vira pedido porque não
        // vira fala. O bloco 1 do teste acima já provou; aqui, a página inteira.
        let only_pictures = "<div><img src=\"0001.png\"/></div>\n\n<hr/>";
        assert!(utterances(only_pictures).is_empty());
        assert!(utterances("").is_empty());
    }

    #[test]
    fn markup_never_reaches_the_synthesizer() {
        // O modelo de voz recebe texto. Uma tag que vazasse seria lida em voz
        // alta, letra por letra.
        let page = "<p>Fã de <em>suspense</em>, descobriu <strong>O Código</strong>.</p>";
        let out = utterances(page);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "Fã de suspense, descobriu O Código.");
        assert!(!out[0].text.contains('<'));
    }

    #[test]
    fn the_sentence_boundary_is_the_one_the_app_already_had() {
        // Não há segunda definição de frase nesta base: se a paginação mudar de
        // ideia sobre "art. 5" ou "3.14", a fala muda junto, e este teste é o
        // que trava as duas na mesma resposta.
        let text = "O art. 5 diz isso. E 3.14 é pi. Fim!";
        assert_eq!(
            split_sentences(text),
            pagination_split(text),
            "a fala divergiu da paginação"
        );
    }

    /// A mesma divisão, feita direto pelos offsets da paginação.
    fn pagination_split(text: &str) -> Vec<String> {
        let mut bounds = pagination::sentence_starts(text);
        bounds.push(text.len());
        let mut out = Vec::new();
        let mut start = 0;
        for end in bounds {
            if end <= start {
                continue;
            }
            let piece = text[start..end].trim();
            if !piece.is_empty() {
                out.push(piece.to_string());
            }
            start = end;
        }
        out
    }
}
