//! The Go implementation's context markers (its goldmark context-marker extension under
//! `markup/goldmark`): the lines
//! [`crate::wrap_context`] puts around an included page's text.
//!
//! goldmark parses a marker as an ordinary line, so it shapes the blocks around an include (a
//! marker line opens a paragraph that ends a definition list, continues a paragraph lazily,
//! keeps an indented include inside its container). Then:
//!
//! - the extension's inline parser makes each marker an empty node that also takes the newline
//!   after it;
//! - the extension's AST transformer (priority 10: before the attribute and image
//!   transformers) replaces a paragraph that holds only a marker by the marker, and drops the
//!   soft line break before a marker in a paragraph. Paragraphs goldmark already made text
//!   blocks (tight list items, the first paragraph of a tight definition) are not paragraphs to
//!   it, so there the line break before a closing marker stays (`…text\n</li>`).
//!
//! Markers are empty `Raw` nodes with [`Role::Context`] (they still count as siblings, as
//! goldmark's nodes do). The pages a marker opens come from the source-context spans, not
//! from the markers.

use comrak::nodes::NodeValue;

use super::{Piece, fill_text, split_text, texts};
use crate::doc::{Doc, Node, Role};
use crate::source::{CONTEXT_CLOSE, CONTEXT_OPEN, strip_context_markers};

/// The marker ranges in `s`, in order.
fn markers(s: &str) -> Vec<std::ops::Range<usize>> {
    let mut out: Vec<_> = [CONTEXT_OPEN, CONTEXT_CLOSE]
        .iter()
        .flat_map(|m| s.match_indices(m).map(|(i, m)| i..i + m.len()))
        .collect();
    out.sort_by_key(|r| r.start);
    out
}

/// Turns the context markers of `doc` into empty nodes with the Go implementation's semantics.
pub(crate) fn contexts(doc: &mut Doc<'_>) {
    if !doc.src.text.contains(CONTEXT_OPEN) && !doc.src.text.contains(CONTEXT_CLOSE) {
        return;
    }
    // Inside code and raw HTML a marker is text (Go strips it from HTML blocks; a code block
    // would show it, which is not reproduced).
    for n in doc.root.descendants() {
        let mut d = n.data_mut();
        match &mut d.value {
            NodeValue::CodeBlock(c) => c.literal = strip_context_markers(&c.literal).into_owned(),
            NodeValue::HtmlBlock(h) => h.literal = strip_context_markers(&h.literal).into_owned(),
            NodeValue::Code(c) => c.literal = strip_context_markers(&c.literal).into_owned(),
            _ => {}
        }
    }
    let mut nodes: Vec<Node<'_>> = Vec::new();
    for t in texts(doc.root, |_| false) {
        let Some(literal) = crate::doc::text_of(t) else {
            continue;
        };
        let found = markers(&literal);
        if found.is_empty() {
            continue;
        }
        let pieces = found
            .into_iter()
            .map(|r| {
                (
                    r,
                    Piece::Node(NodeValue::Raw(String::new()), Some(Role::Context)),
                )
            })
            .collect();
        let created = split_text(doc, t, fill_text(literal.len(), pieces));
        nodes.extend(
            created
                .into_iter()
                .filter(|n| matches!(doc.role(n), Some(Role::Context))),
        );
    }
    // The marker takes the newline after it.
    for m in &nodes {
        if let Some(next) = m
            .next_sibling()
            .filter(|n| matches!(n.data().value, NodeValue::SoftBreak))
        {
            next.detach();
        }
    }
    // The Go extension's AST transformer.
    for m in nodes {
        let Some(p) = m.parent() else { continue };
        if !matches!(p.data().value, NodeValue::Paragraph) || doc.text_block(p) {
            continue;
        }
        if p.children().count() == 1 {
            m.detach();
            p.insert_before(m);
            p.detach();
        } else if let Some(prev) = m
            .previous_sibling()
            .filter(|n| matches!(n.data().value, NodeValue::SoftBreak))
        {
            prev.detach();
        }
    }
}
