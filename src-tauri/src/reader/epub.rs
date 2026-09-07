// SPEC: book-reader (READ-09)

use crate::rag::parsing::ParseError;
use std::io::Read;
use std::path::Path;

/// The tags that end a line when XHTML is flattened. Everything else (`<b>`,
/// `<span>`, `<a>`…) is dropped in place, so a styled word stays glued to the
/// sentence it belongs to instead of becoming its own paragraph.
const BLOCK_TAGS: [&str; 15] = [
    "p", "div", "br", "li", "h1", "h2", "h3", "h4", "h5", "h6", "tr", "blockquote", "section",
    "hr", "body",
];

/// Extracts an EPUB's text in **spine order** (READ-09).
///
/// The order of the zip entries is not the reading order — an EPUB whose
/// chapters are stored as `c, a, b` can perfectly well be read `b, a, c`. So
/// the route is always `META-INF/container.xml` → the `.opf` → `manifest` +
/// `spine`, and any break along it is an error: falling back to zip order
/// would silently hand the reader a shuffled book.
pub fn extract_epub_text(path: &Path) -> Result<String, ParseError> {
    let file = std::fs::File::open(path).map_err(|e| ParseError::ReadFailed(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| ParseError::ReadFailed(e.to_string()))?;

    let container = read_entry(&mut zip, "META-INF/container.xml")?;
    let opf_path = elements(&container, "rootfile")
        .iter()
        .find_map(|tag| attr(tag, "full-path"))
        .ok_or_else(|| {
            ParseError::ReadFailed(
                "EPUB inválido: META-INF/container.xml não aponta para nenhum .opf".to_string(),
            )
        })?;

    let opf = read_entry(&mut zip, &opf_path)?;
    let manifest: Vec<(String, String)> = elements(&opf, "item")
        .iter()
        .filter_map(|tag| Some((attr(tag, "id")?, attr(tag, "href")?)))
        .collect();
    let spine: Vec<String> = elements(&opf, "itemref")
        .iter()
        .filter_map(|tag| attr(tag, "idref"))
        .collect();
    if spine.is_empty() {
        return Err(ParseError::ReadFailed(
            "EPUB inválido: o .opf não declara spine, e a ordem de leitura não pode ser adivinhada"
                .to_string(),
        ));
    }

    // hrefs in the manifest are relative to the .opf, not to the zip root.
    let base = opf_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");

    let mut out = String::new();
    for idref in &spine {
        let href = manifest
            .iter()
            .find(|(id, _)| id == idref)
            .map(|(_, href)| href.clone())
            .ok_or_else(|| {
                ParseError::ReadFailed(format!(
                    "EPUB inválido: o spine cita '{idref}', que não está no manifest"
                ))
            })?;
        let text = xhtml_to_text(&read_entry(&mut zip, &join_path(base, &href))?);
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&text);
    }

    if out.trim().is_empty() {
        return Err(ParseError::NoTextFound);
    }
    Ok(out)
}

fn read_entry<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String, ParseError> {
    let mut entry = zip.by_name(name).map_err(|e| {
        ParseError::ReadFailed(format!("EPUB inválido: '{name}' não pôde ser lido: {e}"))
    })?;
    let mut buf = String::new();
    entry
        .read_to_string(&mut buf)
        .map_err(|e| ParseError::ReadFailed(format!("EPUB inválido: '{name}' não é texto: {e}")))?;
    Ok(buf)
}

/// ponytail: hrefs are used as stored. A percent-encoded href
/// (`Text/a%20b.xhtml`) will miss its zip entry and fail loudly rather than
/// reorder anything; decode it here if T13 measures a real EPUB that needs it.
fn join_path(base: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for segment in href.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Every `<name ...>` in the document, as the raw tag body, in document order.
/// Same linear scan as `rejoin_hyphenated_words`: no XML crate is pulled in for
/// three attributes.
fn elements<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        let after = &rest[lt + 1..];
        let Some(gt) = after.find('>') else { break };
        let tag = &after[..gt];
        rest = &after[gt + 1..];
        if tag_name(tag) == name {
            found.push(tag);
        }
    }
    found
}

/// The element name, lowercased and stripped of its namespace prefix, so
/// `<opf:item>` and `<ITEM>` both answer to "item".
fn tag_name(tag: &str) -> String {
    let tag = tag.trim_start_matches('/').trim_start();
    let name = tag.split([' ', '\t', '\n', '\r', '/']).next().unwrap_or("");
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

/// The value of `name=` inside a tag body. The match requires whitespace before
/// the name, so `href` does not also answer for `xlink:href` and `id` does not
/// answer for `idref`.
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut offset = 0;
    while let Some(i) = tag[offset..].find(name) {
        let at = offset + i;
        offset = at + name.len();
        if at == 0 || !tag.as_bytes()[at - 1].is_ascii_whitespace() {
            continue;
        }
        let after = tag[offset..].trim_start();
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let end = value[1..].find(quote)?;
        return Some(decode_entities(&value[1..1 + end]));
    }
    None
}

/// Flattens XHTML to text: tags out, entities decoded, one paragraph per block
/// boundary. No HTML crate — the design's call, to be revisited only if T13
/// measures a real EPUB that this loses.
fn xhtml_to_text(xhtml: &str) -> String {
    let mut raw = String::with_capacity(xhtml.len());
    let mut rest = xhtml;
    while let Some(lt) = rest.find('<') {
        raw.push_str(&decode_entities(&rest[..lt]));
        let after = &rest[lt + 1..];
        if let Some(comment) = after.strip_prefix("!--") {
            rest = comment.find("-->").map(|i| &comment[i + 3..]).unwrap_or("");
            continue;
        }
        let Some(gt) = after.find('>') else {
            // An unterminated '<' means the rest is not text we can trust.
            rest = "";
            break;
        };
        let name = tag_name(&after[..gt]);
        rest = &after[gt + 1..];
        // <head> holds the document title and metadata, <style>/<script> hold
        // CSS and JS — none of it is prose. Measured while writing the tests:
        // without the <head> skip, every chapter opened with its <title> as a
        // paragraph of its own.
        if name == "head" || name == "style" || name == "script" {
            let close = format!("</{name}");
            rest = find_ci(rest, &close)
                .and_then(|i| rest[i..].find('>').map(|j| &rest[i + j + 1..]))
                .unwrap_or("");
            continue;
        }
        if BLOCK_TAGS.contains(&name.as_str()) {
            raw.push('\n');
        }
    }
    raw.push_str(&decode_entities(rest));

    // Source indentation is layout, not content: collapse it, and let each
    // block boundary become a blank line so paragraphs survive as paragraphs.
    raw.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// ASCII-only lowercasing keeps byte lengths identical, so an index found in
/// the lowered copy is valid in the original.
fn find_ci(haystack: &str, lowercase_needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(lowercase_needle)
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        // A bare '&' in the prose must not swallow the rest of the chapter
        // looking for a ';', so the body is bounded to entity length.
        match after.find(';').filter(|end| *end <= 12) {
            Some(end) => match entity(&after[..end]) {
                Some(c) => {
                    out.push(c);
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('&');
                    rest = after;
                }
            },
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(body: &str) -> Option<char> {
    if let Some(number) = body.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "ndash" => '–',
        "mdash" => '—',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "hellip" => '…',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    //! ⚠️ NÃO LEIA ESTES TESTES COMO PROVA DE QUE UM EPUB REAL FUNCIONA.
    //!
    //! Todos os fixtures abaixo são EPUBs **sintéticos**, montados aqui pelo
    //! crate `zip`, com o XHTML que este arquivo espera. Nenhum EPUB de verdade
    //! passou por este módulo. O que eles provam é exatamente uma coisa: dado um
    //! container/opf bem formado, o texto sai **na ordem do spine** e nunca na
    //! ordem do zip (READ-09), e um container/opf quebrado vira erro em vez de
    //! um livro embaralhado.
    //!
    //! O que eles NÃO provam, e que o Crítico do council levantou: XHTML
    //! malformado de exportador real, CSS embutido de verdade, entidades
    //! exóticas, hrefs percent-encoded. Isso é medido na T13, contra um EPUB
    //! real. Até lá, esta suíte é verde e inconclusiva quanto a EPUB real.

    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("readme-reader-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes the entries in the order given, which is the order the zip stores
    /// them in — that is the whole point of the spine-order test.
    fn write_epub(path: &Path, entries: &[(&str, &str)]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in entries {
            writer.start_file::<_, ()>(*name, opts).unwrap();
            writer.write_all(body.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    fn opf(manifest: &[(&str, &str)], spine: &[&str]) -> String {
        let items: String = manifest
            .iter()
            .map(|(id, href)| {
                format!(r#"<item id="{id}" href="{href}" media-type="application/xhtml+xml"/>"#)
            })
            .collect();
        let refs: String = spine
            .iter()
            .map(|id| format!(r#"<itemref idref="{id}"/>"#))
            .collect();
        format!(
            r#"<?xml version="1.0"?><package version="3.0" xmlns="http://www.idpf.org/2007/opf">
<manifest>{items}</manifest><spine>{refs}</spine></package>"#
        )
    }

    fn chapter(body: &str) -> String {
        format!("<html><head><title>t</title></head><body><p>{body}</p></body></html>")
    }

    #[test]
    fn chapters_come_out_in_spine_order_not_zip_order() {
        // READ-09, the central proof: the zip holds c, a, b — and the spine
        // asks for b, a, c. Alphabetical order, zip order and spine order are
        // three different answers here, on purpose.
        let path = temp_dir("spine").join("livro.epub");
        let package = opf(
            &[("ca", "c.xhtml"), ("aa", "a.xhtml"), ("ba", "b.xhtml")],
            &["ba", "aa", "ca"],
        );
        write_epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", &package),
                ("OEBPS/c.xhtml", &chapter("terceiro no zip")),
                ("OEBPS/a.xhtml", &chapter("segundo no zip")),
                ("OEBPS/b.xhtml", &chapter("primeiro no zip")),
            ],
        );

        let text = extract_epub_text(&path).unwrap();
        assert_eq!(
            text, "primeiro no zip\n\nsegundo no zip\n\nterceiro no zip",
            "a saída seguiu a ordem do zip, não a do spine"
        );
    }

    #[test]
    fn tags_are_stripped_and_paragraphs_survive() {
        let path = temp_dir("tags").join("livro.epub");
        let package = opf(&[("c1", "c1.xhtml")], &["c1"]);
        let body = r#"<html><head><style>p { color: red; }</style></head>
<body>
  <h1>Capítulo <span>um</span></h1>
  <p>Primeiro <b>parágrafo</b> com <i>ênfase</i>.</p>
  <!-- um comentário que não é texto -->
  <p>Segundo parágrafo.</p>
</body></html>"#;
        write_epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", &package),
                ("OEBPS/c1.xhtml", body),
            ],
        );

        let text = extract_epub_text(&path).unwrap();
        assert_eq!(
            text,
            "Capítulo um\n\nPrimeiro parágrafo com ênfase.\n\nSegundo parágrafo."
        );
        // The inline CSS must not reach the reader as prose.
        assert!(
            !text.contains("color"),
            "o CSS do <style> virou texto: {text}"
        );
    }

    #[test]
    fn an_epub_without_container_xml_is_an_error() {
        // Never fall back to zip order: a shuffled book that opens is worse
        // than a book that refuses to open (spec, Edge Cases).
        let path = temp_dir("no-container").join("livro.epub");
        let package = opf(&[("c1", "c1.xhtml")], &["c1"]);
        write_epub(
            &path,
            &[
                ("OEBPS/content.opf", &package),
                ("OEBPS/c1.xhtml", &chapter("texto que não pode sair")),
            ],
        );

        let err = extract_epub_text(&path).unwrap_err();
        assert!(
            matches!(err, ParseError::ReadFailed(_)),
            "esperava ReadFailed, veio {err:?}"
        );
    }

    #[test]
    fn an_opf_without_a_spine_is_an_error() {
        let path = temp_dir("no-spine").join("livro.epub");
        let package = opf(&[("c1", "c1.xhtml")], &[]);
        write_epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", &package),
                ("OEBPS/c1.xhtml", &chapter("texto que não pode sair")),
            ],
        );

        let err = extract_epub_text(&path).unwrap_err();
        assert!(
            matches!(err, ParseError::ReadFailed(_)),
            "esperava ReadFailed, veio {err:?}"
        );
    }

    #[test]
    fn entities_are_decoded() {
        let path = temp_dir("entities").join("livro.epub");
        let package = opf(&[("c1", "c1.xhtml")], &["c1"]);
        write_epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", &package),
                (
                    "OEBPS/c1.xhtml",
                    &chapter(
                        "Fulano &amp; Cia &lt;lida&gt; disse: it&#8217;s &#x201c;ok&#x201d; &naoexiste; 5 &amp; 6",
                    ),
                ),
            ],
        );

        let text = extract_epub_text(&path).unwrap();
        assert_eq!(
            text,
            "Fulano & Cia <lida> disse: it\u{2019}s \u{201c}ok\u{201d} &naoexiste; 5 & 6"
        );
    }
}
