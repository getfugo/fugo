//! The Go implementation's passes over comrak's tree, run between parsing and rendering.

pub(crate) mod blocks;
pub(crate) mod contexts;
pub(crate) mod ids;
pub(crate) mod inline;
pub(crate) mod linkify;
pub(crate) mod tables;
pub(crate) mod typographer;

use std::ops::Range;

use comrak::nodes::NodeValue;

use crate::doc::{Doc, Node, Role, shifted};

/// A piece of a split `Text` node: its byte range in the text, and either plain text or a
/// new inline node.
pub(crate) enum Piece {
    Text,
    Node(NodeValue, Option<Role>),
}

/// Replaces text node `n` by `pieces` (ranges of its literal, in order, covering it).
pub(crate) fn split_text<'a>(
    doc: &mut Doc<'a>,
    n: Node<'a>,
    pieces: Vec<(Range<usize>, Piece)>,
) -> Vec<Node<'a>> {
    let Some(literal) = crate::doc::text_of(n) else {
        return Vec::new();
    };
    let mut created = Vec::with_capacity(pieces.len());
    let sp = n.data().sourcepos;
    let exact = doc.verbatim_at(n, &literal).is_some() && !literal.contains('\n');
    for (range, piece) in pieces {
        let pos = if exact {
            shifted(sp.start, range.start, range.len())
        } else {
            sp
        };
        let (value, role) = match piece {
            Piece::Text => (NodeValue::Text(literal[range].to_owned().into()), None),
            Piece::Node(v, role) => (v, role),
        };
        let new = doc.node(value, pos);
        if let Some(role) = role {
            doc.set_role(new, role);
        }
        n.insert_before(new);
        created.push(new);
    }
    n.detach();
    created
}

/// Text pieces between the ranges of `nodes`: fills the gaps of a sorted, non-overlapping
/// list so that the result covers `0..len`.
pub(crate) fn fill_text(
    len: usize,
    nodes: Vec<(Range<usize>, Piece)>,
) -> Vec<(Range<usize>, Piece)> {
    let mut out = Vec::with_capacity(nodes.len() * 2 + 1);
    let mut at = 0;
    for (r, p) in nodes {
        if r.start > at {
            out.push((at..r.start, Piece::Text));
        }
        at = r.end;
        out.push((r, p));
    }
    if at < len {
        out.push((at..len, Piece::Text));
    }
    out
}

/// The `Text` nodes under `root` in document order, skipping the subtrees `skip` rejects.
pub(crate) fn texts<'a>(root: Node<'a>, skip: impl Fn(&NodeValue) -> bool) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let v = &n.data().value;
        if matches!(v, NodeValue::Text(_)) {
            out.push(n);
            continue;
        }
        if skip(v) {
            continue;
        }
        stack.extend(n.reverse_children());
    }
    out
}
