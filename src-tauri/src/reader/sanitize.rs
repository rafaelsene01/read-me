// SPEC: read-aloud (TTS-09, TTS-10, TTS-11, TTS-36)

//! Taking every executable thing out of the book's markup.
//!
//! This exists because the reader's iframe stops being `sandbox=""` and becomes
//! `sandbox="allow-scripts"` — the app needs its own script inside the page to
//! mark the sentence being read. The sandbox was the defense; now this is.
//!
//! **The change is a downgrade unless this file is right.** Before it,
//! `epub.rs` dropped only a `<script>` that was a top-level block: one nested in
//! a `<div>` survived, harmless only because nothing could run it. That is no
//! longer true.
//!
//! Sanitizing happens at **extraction**, so what is on disk is what runs. The
//! `.html` a reader opens in the explorer (READ-31) is the audited artifact,
//! not a cleaned-up copy made at display time that nobody can inspect.
//!
//! Four shapes are removed, and the test corpus is built from exactly the list
//! the review named as what a hand-written sanitizer gets wrong:
//!
//! 1. `<script>` at any depth, with its content;
//! 2. every `on*` event-handler attribute — `<img onerror>` and `<svg onload>`
//!    fire with no `<script>` anywhere in sight;
//! 3. `javascript:` in `href` and `src`;
//! 4. `<iframe>`, `<object>` and `<embed>`, which load a document of their own.
//!
//! What it must NOT touch is style and structure: the whole point of the page
//! is that it looks like the book (FID-02).

/// Elements removed whole, with everything inside them.
const DROPPED: [&str; 4] = ["script", "iframe", "object", "embed"];

/// Attributes carrying a URL that could be a `javascript:` one.
const URL_ATTRS: [&str; 4] = ["href", "src", "action", "formaction"];

/// Strips every executable thing from one block of the book's markup.
pub fn strip_scripts(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let after = &rest[lt..];

        if let Some(body) = after.strip_prefix("<!--") {
            // A comment is not executable, but it can hide an unbalanced tag
            // from the scan below. Dropping it costs nothing: it is not shown.
            rest = body.find("-->").map(|i| &body[i + 3..]).unwrap_or("");
            continue;
        }
        let Some(gt) = after.find('>') else {
            // An unterminated '<' is not markup we can reason about. It goes
            // out as the literal text it is - and `rest` has to advance past
            // what was already emitted, or the tail is written twice.
            rest = after;
            break;
        };
        let tag = &after[..=gt];
        let name = tag_name(&tag[1..tag.len() - 1]);

        if DROPPED.contains(&name.as_str()) {
            // Skip to the matching close, or to the end. Content included: the
            // body of a `<script>` is the payload, not text.
            rest = skip_element(after, &name, gt + 1);
            continue;
        }
        out.push_str(&clean_tag(tag));
        rest = &after[gt + 1..];
    }
    out.push_str(rest);
    out
}

/// The rest of the input after the element that opens at the start of `input`,
/// whose opening tag ends at `open_end`.
fn skip_element<'a>(input: &'a str, name: &str, open_end: usize) -> &'a str {
    let open_tag = &input[..open_end];
    // `<script/>` and `<script ... />` close themselves.
    if open_tag[..open_tag.len() - 1].trim_end().ends_with('/') {
        return &input[open_end..];
    }
    let close = format!("</{name}");
    let lowered = input[open_end..].to_ascii_lowercase();
    match lowered.find(&close) {
        Some(at) => {
            let from = open_end + at;
            input[from..]
                .find('>')
                .map(|i| &input[from + i + 1..])
                // An unclosed `<script>` swallows the rest, which is the safe
                // direction: better a chapter short than a chapter that runs.
                .unwrap_or("")
        }
        None => "",
    }
}

/// One tag with its dangerous attributes removed. The tag itself, its name and
/// every harmless attribute — `class`, `style`, `id`, `alt` — come out
/// untouched, because the page has to keep looking like the book.
fn clean_tag(tag: &str) -> String {
    let inner = &tag[1..tag.len() - 1];
    let closing_slash = inner.trim_end().ends_with('/');
    let name_end = inner
        .find([' ', '\t', '\n', '\r'])
        .unwrap_or(inner.len());
    let (name, attrs) = inner.split_at(name_end);
    if attrs.trim().is_empty() {
        return tag.to_string();
    }

    let mut out = String::with_capacity(tag.len());
    out.push('<');
    out.push_str(name);
    for (key, whole) in attributes(attrs) {
        let lower = key.to_ascii_lowercase();
        if lower.starts_with("on") {
            continue;
        }
        if URL_ATTRS.contains(&lower.as_str()) && is_script_url(whole) {
            continue;
        }
        out.push(' ');
        out.push_str(whole.trim());
    }
    if closing_slash {
        out.push('/');
    }
    out.push('>');
    out
}

/// `(name, "name=value")` for each attribute in a tag body, quotes respected so
/// a `>` or a space inside a value does not split it.
fn attributes(attrs: &str) -> Vec<(&str, &str)> {
    let bytes = attrs.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' && bytes[i] != b'/' {
            i += 1;
        }
        if i == start {
            break;
        }
        let name = &attrs[start..i];
        let mut end = i;
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'=' {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                j += 1;
                while j < bytes.len() && bytes[j] != quote {
                    j += 1;
                }
                j = (j + 1).min(bytes.len());
            } else {
                while j < bytes.len() && !bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
            }
            end = j;
        }
        out.push((name, &attrs[start..end]));
        i = end.max(i + 1);
    }
    out
}

/// Whether an attribute's value is a URL that executes.
///
/// **The scheme is read by keeping only its letters.** A URL scheme is letters,
/// digits, `+`, `-` and `.` by RFC 3986 - never a tab, never an entity. So
/// everything else before the first `:` is dropped before comparing, and the
/// three classic bypasses collapse onto the same string without decoding
/// anything: `JaVaScRiPt:`, `java\tscript:` and `java&#09;script:` all become
/// `javascript`.
///
/// Written this way on purpose: a decoder would be a second thing to keep
/// correct against every new entity spelling, and the first version of this
/// function tried that and let `java&#09;script:` through - caught by the test
/// corpus, which is exactly what the corpus is for.
fn is_script_url(attribute: &str) -> bool {
    let Some((_, value)) = attribute.split_once('=') else {
        return false;
    };
    let value = value.trim().trim_matches(['"', '\'']);
    let Some((scheme, rest)) = value.split_once(':') else {
        // No scheme at all: a relative link, which cannot execute.
        return false;
    };
    let letters: String = scheme
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();
    letters == "javascript"
        || letters == "vbscript"
        // `data:text/html` renders as a document, so it is a script host too.
        || (letters == "data" && rest.trim_start().to_ascii_lowercase().starts_with("text/html"))
}

fn tag_name(body: &str) -> String {
    let body = body.trim_start_matches('/').trim_start();
    let name = body
        .split([' ', '\t', '\n', '\r', '/'])
        .next()
        .unwrap_or("");
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TTS-36. O corpus é exatamente a lista que a revisão nomeou como o que um
    /// sanitizador escrito à mão erra — e cada linha aqui é uma forma de
    /// executar código **sem** um `<script>` à vista.
    #[test]
    fn every_way_the_book_could_run_code_is_removed() {
        let cases = [
            // 1. script aninhado — o que `epub.rs:82` deixava passar
            (
                r#"<div><p>a</p><script>alert(1)</script></div>"#,
                "script",
            ),
            (r#"<div><SCRIPT SRC="x.js"></SCRIPT></div>"#, "script"),
            (r#"<div><script/></div><p>depois</p>"#, "script"),
            // 2. handler de evento, sem script nenhum
            (r#"<img src="a.png" onerror="alert(1)"/>"#, "onerror"),
            (r#"<svg onload="alert(1)"><circle/></svg>"#, "onload"),
            (r#"<p ONCLICK='alert(1)'>texto</p>"#, "onclick"),
            (r#"<body onload=alert(1)>x</body>"#, "onload"),
            // 3. URL javascript:, inclusive com espaço e maiúscula no meio
            (r#"<a href="javascript:alert(1)">link</a>"#, "javascript"),
            (r#"<a href="  JaVaScRiPt:alert(1)">link</a>"#, "javascript"),
            (r#"<a href="java&#09;script:alert(1)">l</a>"#, "javascript"),
            // 4. elementos que carregam um documento próprio
            (r#"<iframe src="http://x"></iframe>"#, "iframe"),
            (r#"<object data="x.swf"></object>"#, "object"),
            (r#"<embed src="x"/>"#, "embed"),
        ];

        for (input, forbidden) in cases {
            let out = strip_scripts(input).to_ascii_lowercase();
            assert!(
                !out.contains(forbidden),
                "sobrou {forbidden:?} depois de sanitizar {input:?} -> {out:?}"
            );
            assert!(!out.contains("alert(1)"), "sobrou payload em {input:?} -> {out:?}");
        }
    }

    #[test]
    fn a_link_that_merely_contains_a_colon_is_not_a_script_url() {
        // O risco da correção anterior era virar "bloqueia tudo que tem dois
        // pontos", e aí o livro perderia links legítimos.
        for kept in [
            r#"<a href="notas.xhtml#n1">n</a>"#,
            r#"<a href="cap:1/secao:2.xhtml">n</a>"#,
            r#"<a href="https://exemplo.org">n</a>"#,
            r#"<a href="mailto:a@b.c">n</a>"#,
        ] {
            assert_eq!(strip_scripts(kept), kept, "removeu um link legítimo: {kept}");
        }
        // E vbscript entra no mesmo balde do javascript.
        let out = strip_scripts(r#"<a href="vbscript:msgbox(1)">n</a>"#);
        assert!(!out.to_ascii_lowercase().contains("vbscript"), "{out}");
    }

    #[test]
    fn the_book_still_looks_like_the_book() {
        // FID-02 não pode regredir: o sanitizador tira execução, nunca estilo
        // nem estrutura. Este é o teste que impede a correção preguiçosa de
        // "remover tudo que não entendo".
        let input = concat!(
            r#"<div class="capitulo" style="text-align:center">"#,
            r#"<h1 class="titulo">O Arqueiro</h1>"#,
            r#"<p class="versalete">Rebecca <em>Yarros</em></p>"#,
            r#"<img src="0001.png" alt="capa" class="fig"/>"#,
            r#"<a href="notas.xhtml#n1">nota</a>"#,
            "</div>"
        );
        assert_eq!(strip_scripts(input), input, "o sanitizador mexeu na formatação");
    }

    #[test]
    fn a_handler_is_removed_without_taking_its_neighbours() {
        // O caso realista: a tag tem um atributo perigoso no meio de atributos
        // legítimos, e só ele pode sair.
        let out = strip_scripts(r#"<img class="fig" onerror="alert(1)" src="0001.png" alt="capa"/>"#);
        assert!(out.contains(r#"class="fig""#));
        assert!(out.contains(r#"src="0001.png""#));
        assert!(out.contains(r#"alt="capa""#));
        assert!(!out.contains("onerror"));
        assert!(out.ends_with("/>"), "a marca de auto-fechamento sumiu: {out}");
    }

    #[test]
    fn an_unclosed_script_takes_the_rest_instead_of_leaking_it() {
        // Direção segura por escolha: um capítulo a menos é melhor que um
        // capítulo que roda.
        let out = strip_scripts("<p>antes</p><script>alert(1)<p>depois</p>");
        assert_eq!(out, "<p>antes</p>");
    }

    #[test]
    fn ordinary_text_and_entities_pass_through_untouched() {
        assert_eq!(strip_scripts("Fã de &amp; suspense."), "Fã de &amp; suspense.");
        assert_eq!(strip_scripts(""), "");
        // Um `<` solto não pode virar tag na saída.
        assert_eq!(strip_scripts("5 < 7"), "5 < 7");
    }
}
