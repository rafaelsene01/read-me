// SPEC: book-reader (READ-09), book-illustrations (ILLUS-01, ILLUS-08, ILLUS-11),
//       epub-fidelity (FID-01, FID-03, FID-13), read-aloud (TTS-09, TTS-10, TTS-11),
//       book-library (LIB-13)

use super::html;
use super::illustrations::{self, Illustration};
use super::sanitize;
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
    extract_epub(path).map(|(text, _)| text)
}

/// The book with its own markup kept (FID-01, FID-03).
///
/// This is the difference between an EPUB reader and a text extractor: an EPUB
/// **is** HTML+CSS, and the app was throwing that away only to try to rebuild
/// it afterwards. Here the `<body>` of every spine document comes back as a
/// list of blocks with the tags intact, the book's stylesheets come back as
/// one string, and every `<img>` is rewritten to the name its file was stored
/// under.
///
/// `<script>` is dropped on the way out. The reader also renders inside a
/// sandboxed iframe that cannot run it (FID-04) - this is the second lock on
/// the same door, and it costs one comparison.
pub fn extract_epub_html(path: &Path) -> Result<EpubHtml, ParseError> {
    let file = std::fs::File::open(path).map_err(|e| ParseError::ReadFailed(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| ParseError::ReadFailed(e.to_string()))?;
    let (opf_path, manifest, spine) = open_package(&mut zip)?;
    let base = opf_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");

    let mut chapters: Vec<Vec<String>> = Vec::new();
    let mut images: Vec<Illustration> = Vec::new();
    let mut css = String::new();
    let mut css_seen: Vec<String> = Vec::new();

    for idref in &spine {
        let href = href_of(&manifest, idref)?;
        let chapter = join_path(base, &href);
        let xhtml = read_entry(&mut zip, &chapter)?;
        let chapter_dir = chapter.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");

        // The book's stylesheets, once each: two chapters usually share one,
        // and concatenating it twice would double a page of CSS per chapter.
        for link in elements(&xhtml, "link") {
            let is_css = attr(link, "rel").as_deref() == Some("stylesheet")
                || attr(link, "type").as_deref() == Some("text/css");
            let Some(sheet) = attr(link, "href").filter(|_| is_css) else {
                continue;
            };
            let entry = join_path(chapter_dir, &sheet);
            if css_seen.contains(&entry) {
                continue;
            }
            css_seen.push(entry.clone());
            if let Ok(text) = read_entry(&mut zip, &entry) {
                css.push_str(&text);
                css.push('\n');
            }
        }
        for style in inline_styles(&xhtml) {
            css.push_str(&style);
            css.push('\n');
        }

        let mut blocks: Vec<String> = Vec::new();
        for block in html::split_blocks(body_of(&xhtml)) {
            // Sanitizing here, and not at display time, is what makes the
            // `.html` on disk the audited artifact: what the user opens in the
            // explorer (READ-31) is exactly what the iframe runs. It replaced a
            // check that only caught a top-level `<script>` — harmless while
            // the sandbox forbade scripts, and a hole the moment it stopped.
            let block = sanitize::strip_scripts(&block);
            if block.trim().is_empty() {
                continue;
            }
            blocks.push(rewrite_images(
                &block,
                &mut zip,
                chapter_dir,
                &mut images,
            ));
        }
        // One spine document is one chapter (or one piece of front matter),
        // and the boundary is kept so a page never starts a chapter halfway
        // down (FID-13). Measured on the user's "A Última Carta": with the
        // blocks flattened, 53 of its 54 documents began mid-page.
        if !blocks.is_empty() {
            chapters.push(blocks);
        }
    }

    if chapters.is_empty() {
        // Not an error the caller has to hide: `reader_commands` falls back to
        // the text extractor below, so a book this cannot open structurally is
        // still readable.
        return Err(ParseError::NoTextFound);
    }
    Ok(EpubHtml { chapters, css, images })
}

/// An EPUB read as markup: the blocks of each spine document in reading order,
/// the book's CSS, and the image files the blocks point at.
pub struct EpubHtml {
    pub chapters: Vec<Vec<String>>,
    pub css: String,
    pub images: Vec<Illustration>,
}

/// The inner markup of `<body>`, or the whole document when there is no body
/// tag - some exporters ship fragments.
///
/// `pub(crate)` for read-aloud: the page it receives is the assembled document,
/// head included, and speaking starts by throwing that away.
pub(crate) fn body_of(xhtml: &str) -> &str {
    let lower = xhtml.to_ascii_lowercase();
    let Some(open) = lower.find("<body") else {
        return xhtml;
    };
    let Some(gt) = xhtml[open..].find('>').map(|i| open + i + 1) else {
        return xhtml;
    };
    let end = lower[gt..].find("</body>").map(|i| gt + i).unwrap_or(xhtml.len());
    &xhtml[gt..end]
}

/// The bodies of every `<style>` element, which is where a chapter keeps the
/// CSS it does not put in a file.
fn inline_styles(xhtml: &str) -> Vec<String> {
    let lower = xhtml.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = lower[from..].find("<style").map(|i| from + i) {
        let Some(gt) = xhtml[open..].find('>').map(|i| open + i + 1) else {
            break;
        };
        let Some(close) = lower[gt..].find("</style>").map(|i| gt + i) else {
            break;
        };
        out.push(xhtml[gt..close].to_string());
        from = close;
    }
    out
}

/// Rewrites every `src`/`xlink:href` of a block to the name its bytes were
/// stored under, reading each file from the zip once (FID-03).
///
/// An image that cannot be read has its whole `src` left pointing at a name
/// that was never written - so the reader shows nothing for it and the rest of
/// the block survives. Losing one picture must never cost the chapter.
fn rewrite_images<R: Read + std::io::Seek>(
    block: &str,
    zip: &mut zip::ZipArchive<R>,
    chapter_dir: &str,
    images: &mut Vec<Illustration>,
) -> String {
    let mut out = String::with_capacity(block.len());
    let mut rest = block;
    while let Some(lt) = rest.find('<') {
        let Some(gt) = rest[lt..].find('>').map(|i| lt + i + 1) else {
            break;
        };
        let tag = &rest[lt..gt];
        let lower = tag.to_ascii_lowercase();
        if !lower.starts_with("<img") && !lower.starts_with("<image") {
            out.push_str(&rest[..gt]);
            rest = &rest[gt..];
            continue;
        }
        let source = attr(&tag[1..tag.len() - 1], "src")
            .or_else(|| attr(&tag[1..tag.len() - 1], "xlink:href"));
        let replacement = source.as_deref().and_then(|src| {
            let extension = illustrations::extension_of(src)?;
            let bytes = read_binary_entry(zip, &join_path(chapter_dir, src))?;
            let name = illustrations::image_name(images.len() + 1, &extension);
            images.push(Illustration {
                name: name.clone(),
                bytes,
            });
            Some(name)
        });
        match (source, replacement) {
            (Some(src), Some(name)) => {
                out.push_str(&rest[..lt]);
                out.push_str(&tag.replace(&src, &name));
            }
            _ => out.push_str(&rest[..gt]),
        }
        rest = &rest[gt..];
    }
    out.push_str(rest);
    out
}

/// The same walk, keeping the pictures (ILLUS-01).
///
/// Every `<img src>` in a spine document becomes a paragraph of its own -
/// `[[image: NNNN.png]]` - and one entry in the returned list, numbered in
/// order of appearance across the whole book. An `<img>` whose zip entry is
/// missing or unreadable is dropped **together with its marker** (ILLUS-08):
/// a marker naming a file that was never written would show the reader a
/// broken picture instead of nothing.
///
/// A book with no `<img>` at all returns exactly the string this function
/// returned before the feature existed (ILLUS-11).
pub fn extract_epub(path: &Path) -> Result<(String, Vec<Illustration>), ParseError> {
    let file = std::fs::File::open(path).map_err(|e| ParseError::ReadFailed(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| ParseError::ReadFailed(e.to_string()))?;
    let (opf_path, manifest, spine) = open_package(&mut zip)?;
    let base = opf_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    extract_epub_text_inner(&mut zip, base, &manifest, &spine)
}

/// The cover picture's bytes, or `None` when the book declares none (LIB-13).
///
/// Only what the package itself declares: EPUB 3 marks the manifest item with
/// `properties="cover-image"`, EPUB 2 points at it with
/// `<meta name="cover" content="<item id>">`. Nothing is guessed from file
/// names - a wrong guess would show a random plate as the cover, which is
/// worse than the placeholder. Every failure is `None` for the same reason
/// `read_binary_entry` is: a missing cover never costs the row.
pub fn cover_image(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let container = read_entry(&mut zip, "META-INF/container.xml").ok()?;
    let opf_path = elements(&container, "rootfile")
        .iter()
        .find_map(|tag| attr(tag, "full-path"))?;
    let opf = read_entry(&mut zip, &opf_path).ok()?;
    let items = elements(&opf, "item");

    let epub3 = items.iter().find(|tag| {
        attr(tag, "properties").is_some_and(|p| p.split_whitespace().any(|v| v == "cover-image"))
    });
    let epub2 = || {
        let id = elements(&opf, "meta")
            .iter()
            .find(|tag| attr(tag, "name").as_deref() == Some("cover"))
            .and_then(|tag| attr(tag, "content"))?;
        items.iter().find(|tag| attr(tag, "id").as_deref() == Some(id.as_str()))
    };
    let href = attr(epub3.or_else(epub2)?, "href")?;
    // Like every manifest href, relative to the .opf and not to the zip root.
    let base = opf_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    read_binary_entry(&mut zip, &join_path(base, &href))
}

/// `META-INF/container.xml` -> the `.opf` -> `manifest` + `spine`.
///
/// The order of the zip entries is not the reading order, so any break along
/// this route is an error: falling back to zip order would silently hand the
/// reader a shuffled book (READ-09).
type Package = (String, Vec<(String, String)>, Vec<String>);

fn open_package<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>) -> Result<Package, ParseError> {
    let container = read_entry(zip, "META-INF/container.xml")?;
    let opf_path = elements(&container, "rootfile")
        .iter()
        .find_map(|tag| attr(tag, "full-path"))
        .ok_or_else(|| {
            ParseError::ReadFailed(
                "EPUB inválido: META-INF/container.xml não aponta para nenhum .opf".to_string(),
            )
        })?;

    let opf = read_entry(zip, &opf_path)?;
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
    Ok((opf_path, manifest, spine))
}

fn href_of(manifest: &[(String, String)], idref: &str) -> Result<String, ParseError> {
    manifest
        .iter()
        .find(|(id, _)| id == idref)
        .map(|(_, href)| href.clone())
        .ok_or_else(|| {
            ParseError::ReadFailed(format!(
                "EPUB inválido: o spine cita '{idref}', que não está no manifest"
            ))
        })
}

fn extract_epub_text_inner<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    base: &str,
    manifest: &[(String, String)],
    spine: &[String],
) -> Result<(String, Vec<Illustration>), ParseError> {
    let mut out = String::new();
    let mut images: Vec<Illustration> = Vec::new();
    for idref in spine {
        let href = href_of(manifest, idref)?;
        let chapter = join_path(base, &href);
        // `read_entry` hands back an owned String, so the archive is free again
        // by the time the closure below needs it mutably.
        let xhtml = read_entry(zip, &chapter)?;
        // An `<img src>` is relative to the document that holds it, not to the
        // .opf: a chapter in `Text/` pointing at `../Images/fig.png` is the
        // ordinary export shape.
        let chapter_dir = chapter.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        let text = xhtml_to_text(&xhtml, |src| {
            let extension = illustrations::extension_of(src)?;
            let bytes = read_binary_entry(zip, &join_path(chapter_dir, src))?;
            let name = illustrations::image_name(images.len() + 1, &extension);
            images.push(Illustration {
                name: name.clone(),
                bytes,
            });
            Some(illustrations::marker_for(&name))
        });
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
    Ok((out, images))
}

/// The raw bytes of a zip entry, or `None`.
///
/// `None` for every failure on purpose: an `<img>` pointing outside the
/// archive, at a missing entry, or at something unreadable is a dropped
/// picture and never an error that costs the user the whole book (ILLUS-08).
/// The lookup is **by zip entry name**, so a `..` in the href can only miss.
fn read_binary_entry<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<Vec<u8>> {
    let mut entry = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    (!buf.is_empty()).then_some(buf)
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
fn xhtml_to_text(xhtml: &str, mut on_image: impl FnMut(&str) -> Option<String>) -> String {
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
        let tag_body = &after[..gt];
        let name = tag_name(tag_body);
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
        // An illustration is a paragraph: the newlines around the marker
        // are what make it one once the lines below are joined with a blank
        // line between them (ILLUS-04).
        if name == "img" || name == "image" {
            if let Some(marker) = attr(tag_body, "src")
                .or_else(|| attr(tag_body, "xlink:href"))
                .and_then(|src| on_image(&src))
            {
                raw.push('\n');
                raw.push_str(&marker);
                raw.push('\n');
            }
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

pub(super) fn decode_entities(text: &str) -> String {
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

    fn chapter_with_image(body: &str, src: &str) -> String {
        format!(
            "<html><head><title>t</title></head><body><p>{body}</p><img src=\"{src}\"/></body></html>"
        )
    }

    /// Same as `write_epub`, plus binary entries — an image is bytes, and the
    /// text writer would have to pretend otherwise.
    fn write_epub_with_bytes(path: &Path, text: &[(&str, &str)], binary: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in text {
            writer.start_file::<_, ()>(*name, opts).unwrap();
            writer.write_all(body.as_bytes()).unwrap();
        }
        for (name, body) in binary {
            writer.start_file::<_, ()>(*name, opts).unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn images_are_numbered_in_reading_order_across_chapters() {
        // ILLUS-01. Duas imagens em dois capítulos, e o spine pede o segundo
        // capítulo primeiro: a numeração tem de seguir a LEITURA, não o zip.
        let path = temp_dir("illus-order").join("livro.epub");
        let package = opf(&[("a", "a.xhtml"), ("b", "b.xhtml")], &["b", "a"]);
        write_epub_with_bytes(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", &package),
                (
                    "OEBPS/a.xhtml",
                    &chapter_with_image("texto A", "../Images/fig-a.png"),
                ),
                (
                    "OEBPS/b.xhtml",
                    &chapter_with_image("texto B", "Images/fig-b.jpg"),
                ),
            ],
            &[
                ("Images/fig-a.png", b"png-a"),
                ("OEBPS/Images/fig-b.jpg", b"jpg-b"),
            ],
        );

        let (text, images) = extract_epub(&path).unwrap();

        assert_eq!(
            images.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            vec!["0001.jpg", "0002.png"],
            "a numeracao seguiu a ordem do zip em vez da ordem do spine"
        );
        assert_eq!(images[0].bytes, b"jpg-b");
        assert_eq!(images[1].bytes, b"png-a");
        // Cada marcador e um paragrafo proprio, e nessa ordem.
        let paragraphs: Vec<&str> = text.split("\n\n").collect();
        assert_eq!(
            paragraphs,
            vec![
                "texto B",
                "[[image: 0001.jpg]]",
                "texto A",
                "[[image: 0002.png]]"
            ]
        );
    }

    #[test]
    fn an_image_that_cannot_be_read_takes_its_marker_with_it() {
        // ILLUS-08. O href aponta para entrada que nao existe. O livro sai
        // inteiro, sem a gravura E sem um marcador orfao — que na tela seria
        // uma figura quebrada.
        let path = temp_dir("illus-missing").join("livro.epub");
        let package = opf(&[("a", "a.xhtml")], &["a"]);
        write_epub_with_bytes(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", &package),
                (
                    "OEBPS/a.xhtml",
                    &chapter_with_image("texto", "../../fora.png"),
                ),
            ],
            &[],
        );

        let (text, images) = extract_epub(&path).unwrap();

        assert!(images.is_empty(), "leu uma entrada que nao esta no zip");
        assert!(
            !text.contains("[[image:"),
            "sobrou um marcador apontando para arquivo nenhum: {text:?}"
        );
        assert_eq!(text, "texto");
    }

    #[test]
    fn each_spine_document_is_its_own_chapter_in_spine_order() {
        // FID-13. Dois capitulos, o spine pede o segundo primeiro. Achatados
        // num vetor so, a fronteira sumia e a paginacao nao tinha como comecar
        // o capitulo numa pagina nova.
        let path = temp_dir("chapters").join("livro.epub");
        let package = opf(&[("a", "a.xhtml"), ("b", "b.xhtml")], &["b", "a"]);
        write_epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", &package),
                (
                    "OEBPS/a.xhtml",
                    "<html><body><p>a1</p><p>a2</p></body></html>",
                ),
                ("OEBPS/b.xhtml", "<html><body><p>b1</p></body></html>"),
            ],
        );

        let book = extract_epub_html(&path).unwrap();

        assert_eq!(
            book.chapters,
            vec![vec!["<p>b1</p>".to_string()], vec!["<p>a1</p>".to_string(), "<p>a2</p>".to_string()]]
        );
    }

    #[test]
    fn the_cover_is_the_image_the_package_declares_and_nothing_is_guessed() {
        // LIB-13. Um EPUB 3 marca o item; um EPUB 2 aponta por <meta>. Um livro
        // sem declaracao nenhuma fica sem capa, mesmo tendo um item com id
        // "cover" - adivinhar mostraria uma gravura qualquer como capa.
        let dir = temp_dir("cover");
        let manifest_tail = r#"<item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>"#;
        let cases: [(&str, String, Option<&[u8]>); 3] = [
            (
                "epub3.epub",
                format!(
                    r#"<package><metadata/><manifest>{manifest_tail}<item id="img" href="Images/capa.jpg" media-type="image/jpeg" properties="cover-image"/></manifest><spine><itemref idref="c1"/></spine></package>"#
                ),
                Some(&b"capa"[..]),
            ),
            (
                "epub2.epub",
                format!(
                    r#"<package><metadata><meta name="cover" content="img"/></metadata><manifest>{manifest_tail}<item id="img" href="Images/capa.jpg" media-type="image/jpeg"/></manifest><spine><itemref idref="c1"/></spine></package>"#
                ),
                Some(&b"capa"[..]),
            ),
            (
                "sem-capa.epub",
                format!(
                    r#"<package><metadata/><manifest>{manifest_tail}<item id="cover" href="Images/capa.jpg" media-type="image/jpeg"/></manifest><spine><itemref idref="c1"/></spine></package>"#
                ),
                None,
            ),
        ];
        for (name, package, expected) in cases {
            let path = dir.join(name);
            write_epub_with_bytes(
                &path,
                &[
                    ("META-INF/container.xml", CONTAINER),
                    ("OEBPS/content.opf", &package),
                    ("OEBPS/c1.xhtml", &chapter("texto")),
                ],
                &[("OEBPS/Images/capa.jpg", b"capa")],
            );
            assert_eq!(cover_image(&path).as_deref(), expected, "{name}");
        }

        // Um arquivo que nem abre como zip e "sem capa", nao um erro na lista.
        let broken = dir.join("quebrado.epub");
        std::fs::write(&broken, b"nao e zip").unwrap();
        assert_eq!(cover_image(&broken), None);
    }

    #[test]
    fn a_book_without_images_comes_out_exactly_as_it_did_before() {
        // ILLUS-11: a garantia de que esta feature nao mexeu em livro nenhum
        // que nao tenha gravura.
        let path = temp_dir("illus-none").join("livro.epub");
        let package = opf(&[("a", "a.xhtml"), ("b", "b.xhtml")], &["a", "b"]);
        write_epub(
            &path,
            &[
                ("META-INF/container.xml", CONTAINER),
                ("OEBPS/content.opf", &package),
                ("OEBPS/a.xhtml", &chapter("primeiro")),
                ("OEBPS/b.xhtml", &chapter("segundo")),
            ],
        );

        let (text, images) = extract_epub(&path).unwrap();

        assert!(images.is_empty());
        assert_eq!(text, "primeiro\n\nsegundo");
        assert_eq!(extract_epub_text(&path).unwrap(), text);
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
