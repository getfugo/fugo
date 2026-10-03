//! The parsed document: comrak's tree plus what the Go implementation's passes add to its
//! nodes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use comrak::Arena;
use comrak::nodes::{Ast, AstNode, LineColumn, NodeValue, Sourcepos};
use ssg_base::Value;
use ssg_base::diag::Position;

use crate::PassthroughKind;
use crate::source::Prepared;

pub(crate) type Node<'a> = &'a AstNode<'a>;

/// Identity of an arena node (its address; nodes never move).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NodeKey(usize);

impl NodeKey {
    pub(crate) fn of(n: Node<'_>) -> Self {
        Self(std::ptr::from_ref(n).addr())
    }
}

/// What a pass decided about a node beyond comrak's `NodeValue`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Role {
    /// Typographer output (a `Raw` node): HTML written as is.
    Typography(String),
    /// A bare URL or `<…>` autolink (a `Link` node); its label is written unescaped to hooks.
    /// `www` links get `linkifyProtocol` prepended when rendered.
    AutoLink { www: bool },
    /// A passthrough element (a `Raw` node).
    Passthrough {
        kind: PassthroughKind,
        inner: String,
        /// The source including delimiters (the default output).
        raw: String,
    },
    /// An image that replaced its paragraph.
    BlockImage,
    /// A paragraph written without `<p>` (goldmark's text block): the first paragraph of a
    /// tight definition, or the paragraph around a block image.
    TextBlock,
    /// A table cell padded into a short row.
    PaddedCell,
    /// A definition written as tight (`<dd>` directly followed by content).
    TightDetails,
    /// A context marker around an included page's text (an empty `Raw` node).
    Context,
}

/// Per-node data added by passes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Extra {
    /// Attributes in source order (headings: without `id`, which is in `id`).
    pub attrs: Vec<(String, Value)>,
    /// The id of a heading or definition term.
    pub id: Option<String>,
    pub role: Option<Role>,
}

/// A parsed page.
pub(crate) struct Doc<'a> {
    pub arena: &'a Arena<'a>,
    pub root: Node<'a>,
    pub src: Prepared,
    /// The content file (for positions).
    pub file: Arc<Path>,
    extras: HashMap<NodeKey, Extra>,
}

impl<'a> Doc<'a> {
    pub(crate) fn new(
        arena: &'a Arena<'a>,
        root: Node<'a>,
        src: Prepared,
        file: Arc<Path>,
    ) -> Self {
        Self {
            arena,
            root,
            src,
            file,
            extras: HashMap::new(),
        }
    }

    /// The position of `n` in the expanded source.
    pub(crate) fn position(&self, n: Node<'_>) -> Position {
        self.src.position(&self.file, self.original_start(n))
    }

    pub(crate) fn extra(&self, n: Node<'_>) -> Option<&Extra> {
        self.extras.get(&NodeKey::of(n))
    }

    pub(crate) fn extra_mut(&mut self, n: Node<'_>) -> &mut Extra {
        self.extras.entry(NodeKey::of(n)).or_default()
    }

    pub(crate) fn role(&self, n: Node<'_>) -> Option<&Role> {
        self.extra(n).and_then(|e| e.role.as_ref())
    }

    pub(crate) fn set_role(&mut self, n: Node<'_>, role: Role) {
        self.extra_mut(n).role = Some(role);
    }

    /// A new detached node.
    pub(crate) fn node(&self, value: NodeValue, sourcepos: Sourcepos) -> Node<'a> {
        self.arena
            .alloc(AstNode::new(RefCell::new(Ast::new_with_sourcepos(
                value, sourcepos,
            ))))
    }

    /// Whether paragraph `p` is written without `<p>` (goldmark's text blocks: tight list
    /// items, the first paragraph of a tight definition, terms, block images).
    pub(crate) fn text_block(&self, p: Node<'_>) -> bool {
        if matches!(self.role(p), Some(Role::TextBlock)) {
            return true;
        }
        let Some(parent) = p.parent() else {
            return false;
        };
        match &parent.data().value {
            NodeValue::DescriptionTerm => true,
            NodeValue::Item(_) | NodeValue::TaskItem(_) => parent
                .parent()
                .is_some_and(|l| matches!(&l.data().value, NodeValue::List(list) if list.tight)),
            _ => false,
        }
    }

    /// Offset in the parsed text where `n` starts.
    pub(crate) fn start(&self, n: Node<'_>) -> usize {
        self.src.offset(n.data().sourcepos.start)
    }

    /// Offset in the expanded source where `n` starts.
    pub(crate) fn original_start(&self, n: Node<'_>) -> usize {
        self.src.original(self.start(n))
    }

    /// The source line (1-based) of the parsed text.
    pub(crate) fn line(&self, line: usize) -> &str {
        self.src.lines.line(&self.src.text, line)
    }

    /// Whether 1-based `line` is blank (or before the first line).
    pub(crate) fn blank_line(&self, line: usize) -> bool {
        line == 0 || self.line(line).trim().is_empty()
    }

    /// The offset in the parsed text of `n`'s literal when its text is exactly the source
    /// there (plain text without escapes or entities).
    pub(crate) fn verbatim_at(&self, n: Node<'_>, literal: &str) -> Option<usize> {
        let at = self.start(n);
        self.src
            .text
            .get(at..at + literal.len())
            .is_some_and(|s| s == literal)
            .then_some(at)
    }
}

impl Doc<'_> {
    /// The destination and title of an inline link or image as written (escapes and
    /// character references unresolved), which is what Go's hooks see; `None` for
    /// reference links.
    pub(crate) fn raw_link(&self, n: Node<'_>, image: bool) -> Option<(String, String)> {
        let text = &self.src.text;
        let label_end = n
            .descendants()
            .skip(1)
            .map(|d| self.src.offset(d.data().sourcepos.end) + 1)
            .max()
            .unwrap_or_else(|| self.start(n) + if image { 2 } else { 1 });
        let end = (self.src.offset(n.data().sourcepos.end) + 1).min(text.len());
        let s = text.get(label_end..end)?.strip_prefix("](")?;
        let s = s.trim_start();
        let b = s.as_bytes();
        let (dest, rest) = if let Some(inner) = s.strip_prefix('<') {
            let close = inner.find('>')?;
            (&inner[..close], &inner[close + 1..])
        } else {
            let mut depth = 0i32;
            let mut i = 0;
            while i < b.len() {
                match b[i] {
                    b'\\' => i += 1,
                    b'(' => depth += 1,
                    b')' if depth == 0 => break,
                    b')' => depth -= 1,
                    c if c.is_ascii_whitespace() => break,
                    _ => {}
                }
                i += 1;
            }
            let i = i.min(b.len());
            (&s[..i], &s[i..])
        };
        let rest = rest.trim_start();
        let title = match rest.chars().next() {
            Some(q @ ('"' | '\'' | '(')) => {
                let close = if q == '(' { ')' } else { q };
                let body = &rest[1..];
                let mut prev_escape = false;
                let at = body.char_indices().find_map(|(i, c)| {
                    let hit = c == close && !prev_escape;
                    prev_escape = c == '\\' && !prev_escape;
                    hit.then_some(i)
                })?;
                body[..at].to_owned()
            }
            _ => String::new(),
        };
        Some((dest.to_owned(), title))
    }
}

/// A sourcepos `len` bytes after `start` on the same line.
pub(crate) fn shifted(start: LineColumn, by: usize, len: usize) -> Sourcepos {
    let s = LineColumn {
        line: start.line,
        column: start.column + by,
    };
    Sourcepos {
        start: s,
        end: LineColumn {
            line: start.line,
            column: s.column + len.saturating_sub(1),
        },
    }
}

/// The literal of a `Text` node.
pub(crate) fn text_of(n: Node<'_>) -> Option<String> {
    match &n.data().value {
        NodeValue::Text(t) => Some(t.to_string()),
        _ => None,
    }
}
