//! HTML through minify-html, then the code it embeds ([`minify_embedded`]): each `<style>` and
//! `style` attribute through this crate's CSS minifier, with the project's browser targets
//! ([`styles`]), and each inline `<script>` by its type: JavaScript through oxc, JSON through
//! this crate's JSON minifier.

mod styles;

use std::ops::Range;

use oxc_span::SourceType;

use crate::options::{HtmlComments, TemplateSyntax};
use crate::{Minifier, MinifyError, MinifyTarget, js, json, target_for};

pub(crate) fn minify(m: &Minifier, input: &str) -> Result<String, MinifyError> {
    let o = &m.options.html;
    let cfg = minify_html::Cfg {
        keep_comments: o.comments == HtmlComments::KeepAll,
        keep_ssi_comments: o.comments != HtmlComments::Remove,
        keep_closing_tags: o.keep_end_tags,
        keep_html_and_head_opening_tags: o.keep_document_tags,
        keep_input_type_text_attr: o.keep_default_attr_vals,
        preserve_brace_template_syntax: o.templates == TemplateSyntax::Braces,
        preserve_chevron_percent_template_syntax: o.templates == TemplateSyntax::ChevronPercent,
        // Inline CSS and scripts are minified afterwards, by `minify_embedded`.
        minify_css: false,
        minify_js: false,
        // Spec-compliant output only: no `<!doctypehtml>`, no unquoted values with `"'=<>` or
        // backticks, no attributes run together.
        ..minify_html::Cfg::default()
    };
    let utf8 = |b: Vec<u8>| String::from_utf8(b).map_err(|_| MinifyError::HtmlEncoding);
    let (mut out, attributes) =
        minify_embedded(m, &utf8(minify_html::minify(input.as_bytes(), &cfg))?);
    // A second pass makes the result idempotent where minify-html decides on the tokens it has
    // not yet removed: it collapses whitespace before it drops comments (`a <!-- c --> b` leaves
    // two spaces), and it omits an end tag by the next sibling as written (`</tfoot>` before an
    // omitted `</tbody>`). It also writes the `style` attributes minified in between as it writes
    // every attribute (quoted or not, the quoted ones first).
    let dropped_comments = o.comments != HtmlComments::KeepAll && input.contains("<!--");
    if dropped_comments || !o.keep_end_tags || attributes {
        out = utf8(minify_html::minify(out.as_bytes(), &cfg))?;
    }
    Ok(collapse_titles(&out))
}

/// Elements whose content is text, not markup: a `<script>` there is not an element.
const TEXT_ELEMENTS: [&str; 8] = [
    "style",
    "textarea",
    "title",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "plaintext",
];

/// Foreign elements: the text of an SVG or MathML `<script>` is markup (entities, CDATA).
const FOREIGN_ELEMENTS: [&str; 2] = ["svg", "math"];

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// What an inline `<script>` holds, by its `type`.
#[derive(Clone, Copy)]
enum ScriptKind {
    Classic,
    Module,
    Json,
}

impl ScriptKind {
    /// `None` for a type that is neither JavaScript nor JSON (a client-side template such as
    /// `text/x-template`): data the page reads, kept as written.
    fn of(typ: Option<&str>) -> Option<Self> {
        let typ = typ
            .unwrap_or_default()
            .trim_matches(|c: char| u8::try_from(c).is_ok_and(is_space));
        if typ.is_empty() {
            return Some(Self::Classic);
        }
        if typ.eq_ignore_ascii_case("module") {
            return Some(Self::Module);
        }
        if typ.eq_ignore_ascii_case("importmap") || typ.eq_ignore_ascii_case("speculationrules") {
            return Some(Self::Json);
        }
        match target_for(typ)? {
            MinifyTarget::Js => Some(Self::Classic),
            MinifyTarget::Json => Some(Self::Json),
            _ => None,
        }
    }
}

/// `html` with the code it embeds minified: the content of each `<script>` by its type
/// ([`ScriptKind`]), the CSS of each `<style>` and `style` attribute ([`styles`]). Code that does
/// not parse, or that holds preserved template syntax, is kept as written. The flag tells whether
/// a `style` attribute changed.
///
/// minify-html's own JS minification is not used: it prints oxc's `/* @__PURE__ */`
/// annotations, which can make the result longer than the input so that it keeps the input
/// with its line breaks (Google Tag Manager's snippet does this); it parses a classic script as
/// a module, so a global `var` is inlined away; it drops the mangled names; and it leaves JSON
/// (`application/ld+json`) as written. Nor is its CSS minification: it has no browser targets,
/// so it writes the newest syntax (`@media (width>=576px)`, `#rrggbbaa`, `inset`) for every
/// browser.
fn minify_embedded(m: &Minifier, html: &str) -> (String, bool) {
    let lower = html.to_ascii_lowercase();
    let css = m.is_enabled(MinifyTarget::Css);
    let mut out = String::new();
    // `html[..copied]` is in `out`.
    let mut copied = 0;
    let mut replace = |range: Range<usize>, with: &str| {
        out.push_str(&html[copied..range.start]);
        out.push_str(with);
        copied = range.end;
    };
    let mut attributes = false;
    let mut at = 0;
    // The number of open `<svg>`/`<math>` elements.
    let mut foreign = 0_usize;
    while let Some(lt) = lower[at..].find('<').map(|i| at + i) {
        if lower[lt..].starts_with("<!--") {
            at = lower[lt..].find("-->").map_or(lower.len(), |i| lt + i + 3);
            continue;
        }
        // In SVG and MathML, a CDATA section is text.
        if foreign > 0 && lower[lt..].starts_with("<![cdata[") {
            at = lower[lt..].find("]]>").map_or(lower.len(), |i| lt + i + 3);
            continue;
        }
        let Some(tag) = Tag::parse(html, &lower, lt) else {
            at = lt + 1;
            continue;
        };
        at = tag.end;
        if css
            && !tag.closing
            && let Some(style) = tag.style.clone()
            && let Some(min) = styles::attribute(m, &html[style.text])
        {
            replace(style.span, &min);
            attributes = true;
        }
        if FOREIGN_ELEMENTS.contains(&tag.name) {
            if tag.closing {
                foreign = foreign.saturating_sub(1);
            } else if !tag.self_closing {
                foreign += 1;
            }
            continue;
        }
        if tag.closing || (tag.name != "script" && !TEXT_ELEMENTS.contains(&tag.name)) {
            continue;
        }
        let end = lower[at..]
            .find(&format!("</{}", tag.name))
            .map_or(lower.len(), |i| at + i);
        let text = &html[at..end];
        let min = match tag.name {
            "script" if foreign == 0 => minify_script(m, tag.typ, text),
            "style" if css => styles::element(m, text, foreign > 0),
            _ => None,
        };
        if let Some(min) = min {
            replace(at..end, &min);
        }
        at = end;
    }
    if copied == 0 {
        return (html.to_owned(), false);
    }
    out.push_str(&html[copied..]);
    (out, attributes)
}

/// Whether `code` holds template syntax that minify-html preserves.
fn holds_template(m: &Minifier, code: &str) -> bool {
    let delims: &[&str] = match m.options.html.templates {
        TemplateSyntax::None => &[],
        TemplateSyntax::Braces => &["{{", "{%", "{#"],
        TemplateSyntax::ChevronPercent => &["<%"],
    };
    delims.iter().any(|d| code.contains(d))
}

/// The minified `code` of a `<script>` of type `typ`, if it is JavaScript or JSON that parses
/// and the type is enabled.
fn minify_script(m: &Minifier, typ: Option<&str>, code: &str) -> Option<String> {
    if code.is_empty() || holds_template(m, code) {
        return None;
    }
    let js = &m.options.js;
    let min = match ScriptKind::of(typ)? {
        ScriptKind::Classic if m.is_enabled(MinifyTarget::Js) => {
            // oxc's `cjs` is a classic script (no module syntax, sloppy mode).
            js::minify_as(js, SourceType::cjs(), code)
        }
        ScriptKind::Module if m.is_enabled(MinifyTarget::Js) => {
            js::minify_as(js, SourceType::mjs(), code)
        }
        ScriptKind::Json if m.is_enabled(MinifyTarget::Json) => json::minify(code),
        _ => return None,
    }
    .ok()?;
    // The script must still end at its end tag, and must not open an HTML comment, which
    // changes where the end tag is found.
    let lower = min.to_ascii_lowercase();
    (!lower.contains("</script") && !lower.contains("<!--")).then_some(min)
}

/// A start or end tag.
struct Tag<'a> {
    /// The lowercase name.
    name: &'a str,
    closing: bool,
    /// Ends with `/>`.
    self_closing: bool,
    /// The value of the first `type` attribute.
    typ: Option<&'a str>,
    /// The value of the first `style` attribute.
    style: Option<AttrValue>,
    /// The offset after the `>`.
    end: usize,
}

impl<'a> Tag<'a> {
    /// The tag at the `<` at `lt` of `html` (`lower` is its lowercase copy), or `None` when no
    /// complete tag starts there.
    fn parse(html: &'a str, lower: &'a str, lt: usize) -> Option<Self> {
        let b = lower.as_bytes();
        let closing = b.get(lt + 1) == Some(&b'/');
        let start = lt + 1 + usize::from(closing);
        if !b.get(start)?.is_ascii_alphabetic() {
            return None;
        }
        let mut i = start;
        while b
            .get(i)
            .is_some_and(|&c| !is_space(c) && c != b'/' && c != b'>')
        {
            i += 1;
        }
        let name = &lower[start..i];
        let mut self_closing = false;
        let mut typ = None;
        let mut style = None;
        loop {
            match *b.get(i)? {
                b'>' => break,
                b'/' => {
                    self_closing = b.get(i + 1) == Some(&b'>');
                    i += 1;
                }
                c if is_space(c) => i += 1,
                _ => {
                    let attr_start = i;
                    i += 1;
                    while b
                        .get(i)
                        .is_some_and(|&c| !is_space(c) && !matches!(c, b'/' | b'>' | b'='))
                    {
                        i += 1;
                    }
                    let attr = &lower[attr_start..i];
                    while b.get(i).is_some_and(|&c| is_space(c)) {
                        i += 1;
                    }
                    if b.get(i) != Some(&b'=') {
                        continue;
                    }
                    i += 1;
                    while b.get(i).is_some_and(|&c| is_space(c)) {
                        i += 1;
                    }
                    let value_start = i;
                    let text = match *b.get(i)? {
                        q @ (b'"' | b'\'') => {
                            let close = i + 1 + lower[i + 1..].find(char::from(q))?;
                            i = close + 1;
                            value_start + 1..close
                        }
                        _ => {
                            while b.get(i).is_some_and(|&c| !is_space(c) && c != b'>') {
                                i += 1;
                            }
                            value_start..i
                        }
                    };
                    match attr {
                        "type" if typ.is_none() => typ = Some(&html[text]),
                        "style" if style.is_none() => {
                            style = Some(AttrValue {
                                span: value_start..i,
                                text,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
        Some(Self {
            name,
            closing,
            self_closing,
            typ,
            style,
            end: i + 1,
        })
    }
}

/// Where the value of an attribute is written.
#[derive(Clone)]
struct AttrValue {
    /// The value with its quotes.
    span: Range<usize>,
    /// Between the quotes.
    text: Range<usize>,
}

/// `html` with the whitespace of every `<title>` collapsed and trimmed, as Go's minifier writes
/// it (a title is text; browsers and search engines read it that way). minify-html keeps the
/// layout's line breaks and indentation there.
fn collapse_titles(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while let Some(open) = lower[at..].find("<title").map(|i| at + i) {
        let after = lower.as_bytes().get(open + 6).copied();
        let Some(gt) = (matches!(after, Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')))
            .then(|| lower[open..].find('>'))
            .flatten()
            .map(|i| open + i + 1)
        else {
            out.push_str(&html[at..open + 6]);
            at = open + 6;
            continue;
        };
        let Some(close) = lower[gt..].find("</title").map(|i| gt + i) else {
            break;
        };
        out.push_str(&html[at..gt]);
        out.push_str(
            &html[gt..close]
                .split_ascii_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        );
        at = close;
    }
    out.push_str(&html[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::collapse_titles;

    #[test]
    fn titles_are_collapsed() {
        assert_eq!(
            collapse_titles("<head><title>\n   Snack  Diary ·\n Example\n </title></head>"),
            "<head><title>Snack Diary · Example</title></head>"
        );
        assert_eq!(
            collapse_titles(
                "<title lang=th> ไทย  ก </title><titles>x  y</titles><svg><title>a\nb</title></svg>"
            ),
            "<title lang=th>ไทย ก</title><titles>x  y</titles><svg><title>a b</title></svg>"
        );
        assert_eq!(collapse_titles("<title>unclosed  x"), "<title>unclosed  x");
    }
}
