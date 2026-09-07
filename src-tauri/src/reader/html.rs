// SPEC: epub-fidelity (FID-05, FID-06, FID-07, FID-08)

//! Working with the book's own markup instead of throwing it away.
//!
//! Three jobs, one file because they share a scanner and nothing else uses
//! them: cutting a document into **blocks** (FID-05), reading the visible text
//! of a block (the budget and the translation both need it), and swapping the
//! inline tags of a block for placeholders so a 7B model can translate the
//! sentence without being handed HTML to close (FID-06/FID-07).
//!
//! No HTML crate, for the same reason `epub.rs` has none: this parses markup
//! the app itself extracted from the spine, and a linear scan is what the file
//! next door already does. The one thing it must never do is produce invalid
//! markup - that is what `rebuild`'s validation is for.

use super::epub::decode_entities;

/// Elements that never have a closing tag. A block that is one of these is one
/// tag long, and looking for `</img>` would swallow the rest of the chapter.
const VOID_TAGS: [&str; 9] = [
    "br", "hr", "img", "input", "meta", "link", "source", "col", "area",
];

/// Characters per page, from `pagination`: the budget is the same whether the
/// page is text or markup, and it is counted over the **visible text** so the
/// markup does not eat the reader's page.
use super::pagination::PAGE_BUDGET_CHARS;

/// The top-level elements of a fragment, each one a complete element.
///
/// This is what makes cutting a tag in half structurally impossible (FID-05):
/// a page is a list of these, and a block is never split. Text sitting loose
/// between elements comes back as its own block, so nothing is dropped.
pub fn split_blocks(html: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i] == b'<' {
            let end = element_end(html, i);
            let block = html[i..end].trim();
            if !block.is_empty() {
                blocks.push(block.to_string());
            }
            i = end;
            continue;
        }
        // A loose text node: everything up to the next tag.
        let end = html[i..].find('<').map(|j| i + j).unwrap_or(html.len());
        let text = html[i..end].trim();
        if !text.is_empty() {
            blocks.push(text.to_string());
        }
        i = end;
    }
    blocks
}

/// The byte after the element that starts at `start` (which must be a `<`).
fn element_end(html: &str, start: usize) -> usize {
    let rest = &html[start..];
    if rest.starts_with("<!--") {
        return rest.find("-->").map(|i| start + i + 3).unwrap_or(html.len());
    }
    let Some(gt) = rest.find('>') else {
        return html.len();
    };
    let body = &rest[1..gt];
    let name = tag_name(body);
    let open_end = start + gt + 1;
    // `<?xml ...>`, `<!doctype ...>`, a self-closing tag and a void element all
    // end where their `>` does.
    if body.starts_with('!')
        || body.starts_with('?')
        || body.ends_with('/')
        || VOID_TAGS.contains(&name.as_str())
    {
        return open_end;
    }
    // Otherwise find the matching close, counting nested opens of the same
    // name - `<div><div></div></div>` must not end at the first `</div>`.
    let mut depth = 1;
    let mut i = open_end;
    while i < html.len() {
        let Some(lt) = html[i..].find('<') else { break };
        let at = i + lt;
        let end = element_end_shallow(html, at);
        let tag = html[at + 1..end].trim_end_matches('>');
        let closing = tag.starts_with('/');
        if tag_name(tag) == name && !tag.starts_with('!') {
            if closing {
                depth -= 1;
                if depth == 0 {
                    return end;
                }
            } else if !tag.ends_with('/') {
                depth += 1;
            }
        }
        i = end;
    }
    html.len()
}

/// The byte after the `>` of the tag starting at `start`, without matching
/// anything. Used by the depth counter, which must step tag by tag.
fn element_end_shallow(html: &str, start: usize) -> usize {
    let rest = &html[start..];
    if rest.starts_with("<!--") {
        return rest.find("-->").map(|i| start + i + 3).unwrap_or(html.len());
    }
    rest.find('>').map(|i| start + i + 1).unwrap_or(html.len())
}

/// The element name of a tag body, lowercased, closing slash and namespace
/// prefix removed - `</opf:p ` and `<P` both answer "p".
fn tag_name(body: &str) -> String {
    let body = body.trim_start_matches('/').trim_start();
    let name = body.split([' ', '\t', '\n', '\r', '/', '>']).next().unwrap_or("");
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

/// The text a reader would see: tags out, entities decoded, whitespace
/// collapsed. What the page budget counts and what the model is asked to
/// translate.
pub fn visible_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < html.len() {
        match html[i..].find('<') {
            Some(0) => i = element_end_shallow(html, i),
            Some(lt) => {
                out.push_str(&html[i..i + lt]);
                i += lt;
            }
            None => {
                out.push_str(&html[i..]);
                break;
            }
        }
    }
    decode_entities(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Groups blocks into pages of at most [`PAGE_BUDGET_CHARS`] visible
/// characters, in order (FID-05).
///
/// A block wider than the whole budget becomes a page by itself: cutting
/// inside it would mean understanding the tree, and READ-10 already prefers an
/// oversized page to a split paragraph. Each page is its blocks joined by a
/// blank line, so `split_blocks` on a page gives the blocks back - that is the
/// shape the old `paginate(t).concat() == t` invariant takes here.
pub fn paginate_blocks(blocks: &[String]) -> Vec<String> {
    let mut pages: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut spent = 0;
    for block in blocks {
        let cost = visible_text(block).chars().count();
        if !current.is_empty() && spent + cost > PAGE_BUDGET_CHARS {
            pages.push(current.join("\n\n"));
            current.clear();
            spent = 0;
        }
        current.push(block);
        spent += cost;
    }
    if !current.is_empty() {
        pages.push(current.join("\n\n"));
    }
    pages
}

/// A block taken apart for translation: the outer tag stays, the inline tags
/// become numbered placeholders, and the text in between is what the model
/// sees (FID-06).
pub struct Placeheld {
    /// The block's own opening tag, `""` for a loose text block.
    open: String,
    close: String,
    /// The inner text with `⟦n⟧ … ⟦/n⟧` where an inline element was.
    pub text: String,
    /// Per index, the opening tag to put back and its closing tag.
    tags: Vec<(String, String)>,
}

const MARK_OPEN: char = '\u{27e6}'; // ⟦
const MARK_CLOSE: char = '\u{27e7}'; // ⟧

pub fn placehold(block: &str) -> Placeheld {
    let Some((open, inner, close)) = peel(block) else {
        return Placeheld {
            open: String::new(),
            close: String::new(),
            text: decode_entities(block).split_whitespace().collect::<Vec<_>>().join(" "),
            tags: Vec::new(),
        };
    };

    let mut text = String::new();
    let mut tags: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < inner.len() {
        match inner[i..].find('<') {
            Some(0) => {
                let end = element_end(inner, i);
                let element = &inner[i..end];
                let n = tags.len() + 1;
                match peel(element) {
                    Some((tag_open, tag_inner, tag_close)) => {
                        tags.push((tag_open.to_string(), tag_close.to_string()));
                        text.push(MARK_OPEN);
                        text.push_str(&n.to_string());
                        text.push(MARK_CLOSE);
                        // One level deep on purpose: a nested inline inside an
                        // inline is rare, and flattening it costs styling, not
                        // the sentence. Deeper nesting would need a tree.
                        text.push_str(&visible_text(tag_inner));
                        text.push(MARK_OPEN);
                        text.push('/');
                        text.push_str(&n.to_string());
                        text.push(MARK_CLOSE);
                    }
                    // Void or self-closing (`<br/>`, `<img/>`): an empty pair,
                    // so it survives the round trip with no content of its own.
                    None => {
                        tags.push((element.to_string(), String::new()));
                        text.push(MARK_OPEN);
                        text.push_str(&n.to_string());
                        text.push(MARK_CLOSE);
                        text.push(MARK_OPEN);
                        text.push('/');
                        text.push_str(&n.to_string());
                        text.push(MARK_CLOSE);
                    }
                }
                i = end;
            }
            Some(lt) => {
                text.push_str(&decode_entities(&inner[i..i + lt]));
                i += lt;
            }
            None => {
                text.push_str(&decode_entities(&inner[i..]));
                break;
            }
        }
    }

    Placeheld {
        open: open.to_string(),
        close: close.to_string(),
        text: text.trim().to_string(),
        tags,
    }
}

/// Puts `translated` back inside the block's own tag, restoring the inline
/// tags the placeholders stand for.
///
/// **The validation is the point (FID-07).** If the model dropped a marker,
/// duplicated one, invented one, or closed one before opening it, the block
/// comes back as plain escaped text inside its own tag: correct words, no
/// styling. It never emits a tag the model invented and never an orphan
/// `</em>` - a broken tag would land in a file on disk and stay there.
pub fn rebuild(held: &Placeheld, translated: &str) -> String {
    let inner = restore(&held.tags, translated)
        .unwrap_or_else(|| escape(&strip_markers(translated)));
    format!("{}{}{}", held.open, inner, held.close)
}

fn restore(tags: &[(String, String)], translated: &str) -> Option<String> {
    let mut out = String::with_capacity(translated.len());
    let mut seen = vec![0u8; tags.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut rest = translated;
    while let Some(at) = rest.find(MARK_OPEN) {
        out.push_str(&escape(&rest[..at]));
        let after = &rest[at + MARK_OPEN.len_utf8()..];
        let end = after.find(MARK_CLOSE)?;
        let body = &after[..end];
        rest = &after[end + MARK_CLOSE.len_utf8()..];

        let (closing, digits) = match body.strip_prefix('/') {
            Some(digits) => (true, digits),
            None => (false, body),
        };
        let n: usize = digits.parse().ok()?;
        let index = n.checked_sub(1).filter(|i| *i < tags.len())?;
        if closing {
            if stack.pop() != Some(index) {
                return None;
            }
            out.push_str(&tags[index].1);
        } else {
            if seen[index] != 0 {
                return None;
            }
            seen[index] = 1;
            stack.push(index);
            out.push_str(&tags[index].0);
        }
    }
    out.push_str(&escape(rest));
    // Every tag has to come back, and nothing may be left open.
    (stack.is_empty() && seen.iter().all(|s| *s == 1)).then_some(out)
}

fn strip_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(MARK_OPEN) {
        out.push_str(&rest[..at]);
        let after = &rest[at + MARK_OPEN.len_utf8()..];
        match after.find(MARK_CLOSE) {
            Some(end) => rest = &after[end + MARK_CLOSE.len_utf8()..],
            None => {
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Splits an element into its opening tag, its inner markup and its closing
/// tag. `None` for anything without an inner: a void element, a self-closing
/// tag, or a loose text node.
fn peel(element: &str) -> Option<(&str, &str, &str)> {
    let element = element.trim();
    if !element.starts_with('<') {
        return None;
    }
    let gt = element.find('>')?;
    let body = &element[1..gt];
    if body.ends_with('/') || VOID_TAGS.contains(&tag_name(body).as_str()) {
        return None;
    }
    let close_at = element.rfind("</")?;
    (close_at >= gt).then(|| (&element[..gt + 1], &element[gt + 1..close_at], &element[close_at..]))
}

/// Only the three characters that could start a tag or an entity. The book's
/// own markup is never passed through here - only text that came back from the
/// model.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_is_a_whole_element_even_when_it_nests_its_own_name() {
        let html = r#"<div class="a"><div>dentro</div></div><p>depois</p><img src="x.png"/>"#;
        let blocks = split_blocks(html);
        assert_eq!(
            blocks,
            vec![
                r#"<div class="a"><div>dentro</div></div>"#,
                "<p>depois</p>",
                r#"<img src="x.png"/>"#
            ],
            "o casamento de tags parou na primeira </div>"
        );
    }

    #[test]
    fn visible_text_is_what_the_reader_sees_and_nothing_else() {
        assert_eq!(
            visible_text("<p>F&amp;C de <em>suspense</em>,\n   descobriu.</p>"),
            "F&C de suspense, descobriu."
        );
        assert_eq!(visible_text(r#"<div><img src="a.png"/></div>"#), "");
    }

    #[test]
    fn pages_are_whole_blocks_and_the_blocks_survive_the_round_trip() {
        // FID-05: a forma que a invariante da READ-10 assume aqui — a
        // concatenação é de blocos, não de caracteres.
        let long = "palavra ".repeat(400); // ~3.200 caracteres visíveis
        let blocks: Vec<String> = (0..6)
            .map(|i| format!("<p>{i} {long}</p>"))
            .collect();

        let pages = paginate_blocks(&blocks);

        assert!(pages.len() > 1, "tudo foi para uma página só");
        let back: Vec<String> = pages.iter().flat_map(|p| split_blocks(p)).collect();
        assert_eq!(back, blocks, "a página não devolveu os blocos que recebeu");
        for page in &pages {
            for block in split_blocks(page) {
                assert!(block.starts_with("<p>") && block.ends_with("</p>"));
            }
        }
    }

    #[test]
    fn a_block_bigger_than_the_budget_becomes_a_page_by_itself() {
        let huge = format!("<p>{}</p>", "a ".repeat(PAGE_BUDGET_CHARS));
        let blocks = vec!["<p>antes</p>".to_string(), huge.clone(), "<p>depois</p>".to_string()];

        let pages = paginate_blocks(&blocks);

        assert_eq!(pages, vec!["<p>antes</p>", &huge, "<p>depois</p>"]);
    }

    #[test]
    fn the_model_gets_the_whole_sentence_with_the_inline_tags_as_markers() {
        // FID-06, o caso do próprio livro do usuário.
        let block = "<p>Fã de suspense, descobriu <em>O Código Da Vinci</em> antes do lançamento.</p>";
        let held = placehold(block);

        assert_eq!(
            held.text,
            "Fã de suspense, descobriu ⟦1⟧O Código Da Vinci⟦/1⟧ antes do lançamento.",
            "a requisição não levou a frase inteira"
        );

        let translated = "Fan of suspense, he found ⟦1⟧The Da Vinci Code⟦/1⟧ before release.";
        assert_eq!(
            rebuild(&held, translated),
            "<p>Fan of suspense, he found <em>The Da Vinci Code</em> before release.</p>"
        );
    }

    #[test]
    fn a_broken_placeholder_degrades_to_text_and_never_to_invalid_markup() {
        // FID-07. Quatro maneiras de o modelo errar; nenhuma pode gerar tag.
        let held = placehold("<p>a <em>b</em> c <strong>d</strong></p>");
        for broken in [
            "a ⟦1⟧b c ⟦2⟧d⟦/2⟧",             // ⟦1⟧ nunca fecha
            "a b⟦/1⟧ c ⟦2⟧d⟦/2⟧",            // fecha sem abrir
            "a ⟦1⟧b⟦/1⟧ c d",                 // perdeu o par 2
            "a ⟦9⟧b⟦/9⟧ c ⟦2⟧d⟦/2⟧",         // inventou um índice
        ] {
            let out = rebuild(&held, broken);
            assert!(
                !out.contains("<em") && !out.contains("<strong") && !out.contains("</em"),
                "gerou marcação a partir de placeholder quebrado: {out}"
            );
            assert!(!out.contains('\u{27e6}'), "sobrou marcador na tela: {out}");
            assert!(out.starts_with("<p>") && out.ends_with("</p>"));
        }
    }

    #[test]
    fn text_from_the_model_is_escaped_so_it_can_never_open_a_tag() {
        let held = placehold("<p>x</p>");
        assert_eq!(
            rebuild(&held, "5 < 7 & <script>alert(1)</script>"),
            "<p>5 &lt; 7 &amp; &lt;script&gt;alert(1)&lt;/script&gt;</p>"
        );
    }

    #[test]
    fn a_self_closing_inline_survives_and_a_block_with_no_text_asks_for_nothing() {
        // FID-08: quem decide "não traduzir" é o texto visível estar vazio.
        let held = placehold(r#"<p>uma<br/>duas</p>"#);
        assert_eq!(held.text, "uma⟦1⟧⟦/1⟧duas");
        assert_eq!(rebuild(&held, "one⟦1⟧⟦/1⟧two"), "<p>one<br/>two</p>");

        assert_eq!(visible_text(r#"<div class="fig"><img src="0001.png"/></div>"#), "");
    }
}
