//! Linkify and the typographer in one left-to-right scan per text run, the way goldmark's
//! inline parser interleaves them: a quote turned into a typographic quote lets a link start
//! right after it, and a link swallows the quote that ends it.

use std::ops::Range;

use comrak::nodes::{NodeLink, NodeValue};

use super::linkify;
use super::typographer::{self, Counters};
use super::{Piece, fill_text, split_text, texts};
use crate::Typographer;
use crate::doc::{Doc, Node, Role};

/// Runs linkify (when `linkify`) and the typographer (when `typo`) over every block's text.
pub(crate) fn run(doc: &mut Doc<'_>, typo: Option<&Typographer>, linkify: bool) {
    if typo.is_none() && !linkify {
        return;
    }
    if typo.is_some() {
        unhtml_angle_quotes(doc);
    }
    let blocks: Vec<Node<'_>> = doc
        .root
        .descendants()
        .filter(|n| n.data().value.contains_inlines())
        .collect();
    for block in blocks {
        let mut counters = Counters::default();
        let nodes = texts(block, |v| {
            matches!(
                v,
                NodeValue::Code(_)
                    | NodeValue::HtmlInline(_)
                    | NodeValue::Raw(_)
                    | NodeValue::Escaped
            )
        });
        for n in nodes {
            let link = n
                .ancestors()
                .skip(1)
                .take_while(|a| !a.same_node(block))
                .find(|a| matches!(a.data().value, NodeValue::Link(_) | NodeValue::Image(_)));
            if link.is_some_and(|l| matches!(doc.role(l), Some(Role::AutoLink { .. }))) {
                continue;
            }
            let ctx = Ctx {
                typo,
                linkify: linkify && link.is_none(),
                continued: continued(block, n),
            };
            scan(doc, n, &ctx, &mut counters);
        }
    }
}

/// goldmark's typographer takes `<<` before raw HTML is recognised, so `<<angle>>` is
/// quotes, not a tag: turn such inline HTML back into text.
fn unhtml_angle_quotes(doc: &mut Doc<'_>) {
    let tags: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| {
            matches!(n.data().value, NodeValue::HtmlInline(_))
                && n.previous_sibling()
                    .and_then(crate::doc::text_of)
                    .is_some_and(|t| t.ends_with('<'))
        })
        .collect();
    for tag in tags {
        let NodeValue::HtmlInline(html) = tag.data().value.clone() else {
            continue;
        };
        let Some(prev) = tag.previous_sibling() else {
            continue;
        };
        let mut merged = crate::doc::text_of(prev).unwrap_or_default();
        merged.push_str(&html);
        if let Some(next) = tag.next_sibling()
            && let Some(t) = crate::doc::text_of(next)
        {
            merged.push_str(&t);
            next.detach();
        }
        prev.data_mut().value = NodeValue::Text(merged.into());
        tag.detach();
    }
}

/// How one text run is scanned.
struct Ctx<'t> {
    typo: Option<&'t Typographer>,
    linkify: bool,
    /// The run's source line continues the block (a paragraph line that is not its last):
    /// goldmark then sees the line's newline; the last line is right-trimmed.
    continued: bool,
}

fn continued(block: Node<'_>, n: Node<'_>) -> bool {
    matches!(block.data().value, NodeValue::Paragraph)
        && n.data().sourcepos.start.line < block.data().sourcepos.end.line
}

fn scan<'a>(doc: &mut Doc<'a>, n: Node<'a>, ctx: &Ctx<'_>, counters: &mut Counters) {
    let (typo, linkify) = (ctx.typo, ctx.linkify);
    let Some(lit) = crate::doc::text_of(n) else {
        return;
    };
    let base = doc.verbatim_at(n, &lit);
    let mut found: Vec<(Range<usize>, Piece)> = Vec::new();
    let mut labels = Vec::new();
    let mut i = 0;
    let mut head = true;
    while i < lit.len() {
        if linkify
            && head
            && let Some(f) = linkify::find(&lit[i..])
        {
            labels.push(lit[i..i + f.len].to_owned());
            let link = NodeValue::Link(Box::new(NodeLink {
                url: f.url,
                title: String::new(),
            }));
            found.push((
                i..i + f.len,
                Piece::Node(link, Some(Role::AutoLink { www: f.www })),
            ));
            i += f.len;
            continue;
        }
        let c = lit[i..].chars().next().unwrap_or(' ');
        if let Some(t) = typo
            && typographer::TRIGGERS.contains(&c)
        {
            let src = &doc.src.text;
            let (line, before) = match base {
                Some(at) => {
                    let from = at + i;
                    let end = src[from..].find('\n').map_or(src.len(), |e| from + e);
                    let line = if ctx.continued && end < src.len() {
                        &src[from..=end]
                    } else {
                        src[from..end].trim_end()
                    };
                    (line, src[..from].chars().next_back().unwrap_or('\n'))
                }
                None => (&lit[i..], lit[..i].chars().next_back().unwrap_or('\n')),
            };
            if let Some((len, with)) = typographer::replacement(t, line, before, counters) {
                let len = len.min(lit.len() - i);
                found.push((
                    i..i + len,
                    Piece::Node(
                        NodeValue::Raw(with.to_owned()),
                        Some(Role::Typography(with.to_owned())),
                    ),
                ));
                i += len;
                head = true;
                continue;
            }
        }
        head = c.is_ascii() && linkify::TRIGGERS.contains(&(c as u8));
        i += c.len_utf8();
    }
    if found.is_empty() {
        return;
    }
    let created = split_text(doc, n, fill_text(lit.len(), found));
    let links = created
        .into_iter()
        .filter(|c| matches!(c.data().value, NodeValue::Link(_)));
    for (link, label) in links.zip(labels) {
        let sp = link.data().sourcepos;
        link.append(doc.node(NodeValue::Text(label.into()), sp));
    }
}
