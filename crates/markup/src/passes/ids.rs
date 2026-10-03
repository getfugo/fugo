//! Plain text of inline content (the Go implementation's `TextPlain`) and the ids of headings
//! and definition terms.

use comrak::nodes::NodeValue;
use ssg_base::anchor::{self, Deduper};

use crate::doc::{Doc, Node, Role};
use crate::{Extensions, MarkdownOptions};

/// The Go implementation's plain text of `n`: each direct child's text, where a container
/// child contributes only the plain text of its *first* child (so `**a *b* c**` gives `a `);
/// named entity references resolved.
pub(crate) fn text_plain(doc: &Doc<'_>, n: Node<'_>) -> String {
    let mut s = String::new();
    for c in n.children() {
        plain_to(doc, c, &mut s, false);
    }
    resolve_entities(&s)
}

/// The text of a `Text` node as written in the source (escapes and character references
/// unresolved), when its position allows.
fn source_text(doc: &Doc<'_>, n: Node<'_>, literal: &str) -> String {
    let sp = n.data().sourcepos;
    if sp.start.line != sp.end.line || !literal.contains(|c: char| !c.is_ascii_alphanumeric()) {
        return literal.to_owned();
    }
    let line = doc.line(sp.start.line);
    match line.get(sp.start.column.saturating_sub(1)..sp.end.column.min(line.len())) {
        Some(raw) if raw.contains(['&', '\\']) && raw.len() > literal.len() => raw.to_owned(),
        _ => literal.to_owned(),
    }
}

/// goldmark splits text after a run of emphasis delimiters that closed nothing, so the first
/// text of a container ends there (`_under_score_` gives `under_`).
fn first_segment(t: &str) -> &str {
    let b = t.as_bytes();
    let Some(start) = b.iter().position(|c| matches!(c, b'*' | b'_' | b'~')) else {
        return t;
    };
    let run = b[start..].iter().take_while(|&&c| c == b[start]).count();
    &t[..start + run]
}

fn plain_to(doc: &Doc<'_>, n: Node<'_>, s: &mut String, first: bool) {
    match &n.data().value {
        NodeValue::Text(t) => {
            let raw = source_text(doc, n, t);
            s.push_str(if first { first_segment(&raw) } else { &raw });
        }
        NodeValue::Link(_) if matches!(doc.role(n), Some(Role::AutoLink { .. })) => {}
        NodeValue::SoftBreak | NodeValue::LineBreak => s.push('\n'),
        NodeValue::Raw(r) => match doc.role(n) {
            Some(Role::Typography(v)) => s.push_str(v),
            Some(Role::Passthrough { raw, .. }) => s.push_str(raw),
            _ => s.push_str(r),
        },
        NodeValue::HtmlInline(h) => s.push_str(crate::text::strip_html(h).trim()),
        NodeValue::ShortCode(sc) => s.push_str(&sc.code),
        NodeValue::Code(c) => s.push_str(&c.literal),
        NodeValue::Escaped => {
            s.push('\\');
            for c in n.children() {
                plain_to(doc, c, s, first);
            }
        }
        _ => {
            if let Some(c) = n.first_child() {
                plain_to(doc, c, s, true);
            }
        }
    }
}

/// The named character references of typographer output and common text.
const ENTITIES: &[(&str, &str)] = &[
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", "\u{a0}"),
    ("lsquo", "\u{2018}"),
    ("rsquo", "\u{2019}"),
    ("ldquo", "\u{201c}"),
    ("rdquo", "\u{201d}"),
    ("sbquo", "\u{201a}"),
    ("bdquo", "\u{201e}"),
    ("ndash", "\u{2013}"),
    ("mdash", "\u{2014}"),
    ("hellip", "\u{2026}"),
    ("laquo", "\u{ab}"),
    ("raquo", "\u{bb}"),
    ("lsaquo", "\u{2039}"),
    ("rsaquo", "\u{203a}"),
    ("copy", "\u{a9}"),
    ("reg", "\u{ae}"),
    ("trade", "\u{2122}"),
    ("deg", "\u{b0}"),
    ("times", "\u{d7}"),
    ("minus", "\u{2212}"),
    ("middot", "\u{b7}"),
    ("bull", "\u{2022}"),
];

/// Resolves named character references (`&rsquo;` → `’`); numeric ones stay.
pub(crate) fn resolve_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let resolved = tail.find(';').and_then(|end| {
            let name = &tail[..end];
            ENTITIES
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v, end + 1))
        });
        match resolved {
            Some((v, len)) => {
                out.push_str(v);
                rest = &tail[len..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text a heading id is made from: the plain text, and for a multi-line (setext)
/// heading only its last line.
fn heading_text(doc: &Doc<'_>, h: Node<'_>) -> String {
    let text = text_plain(doc, h);
    match text.rfind('\n') {
        Some(i) => text[i + 1..].to_owned(),
        None => text,
    }
}

/// Gives headings (and definition terms) their ids: explicit ids are kept and reserved,
/// the others are made from the plain text and de-duplicated per document.
pub(crate) fn assign(doc: &mut Doc<'_>, o: &MarkdownOptions) {
    let mut ids = Deduper::new();
    let nodes: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| {
            matches!(
                n.data().value,
                NodeValue::Heading(_) | NodeValue::DescriptionTerm
            )
        })
        .collect();
    let style = o.heading_ids.unwrap_or_default();
    for n in nodes {
        let heading = matches!(n.data().value, NodeValue::Heading(_));
        if let Some(id) = doc.extra(n).and_then(|e| e.id.clone()) {
            ids.reserve(&id);
            continue;
        }
        let (text, fallback) = if heading {
            if o.heading_ids.is_none() {
                continue;
            }
            (heading_text(doc, n), "heading")
        } else {
            if !o.extensions.contains(Extensions::DEFINITION_TERM_IDS) {
                continue;
            }
            let para = n.first_child().unwrap_or(n);
            (text_plain(doc, para), "term")
        };
        if text.is_empty() {
            continue;
        }
        let id = ids.unique(anchor::anchorize(&text, style), fallback);
        doc.extra_mut(n).id = Some(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities() {
        assert_eq!(
            resolve_entities("It&rsquo;s &amp; &#39; &bogus; &"),
            "It\u{2019}s & &#39; &bogus; &"
        );
    }
}
