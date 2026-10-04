//! The HTML renderer: goldmark's output for every node, the Go implementation's renderers
//! (blockquotes, tables, code blocks, footnotes) and the render hooks.
//!
//! The walk is iterative (deeply nested blockquotes and lists do not grow the call stack).
//! Nodes whose hook needs the rendered content record the output length when entered and
//! take everything written after it when left, so nested hooks have already run.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock};

use comrak::nodes::{ListType, NodeValue, TableAlignment};
use regex::Regex;
use ssg_base::{Map, PageId};

use crate::attributes::{self, Owner};
use crate::doc::{Doc, Node, Role};
use crate::escape;
use crate::hooks::{
    AlertSign, Alignment, BlockquoteCtx, BlockquoteKind, Cell, CodeBlockCtx, HeadingCtx,
    HighlightOptions, Highlighter, HookEnv, HookError, HookOut, Hooks, LinkCtx, PassthroughCtx,
    TableCtx,
};
use crate::passes::ids::text_plain;
use crate::source::SourceContexts;
use crate::{
    CodeFences, Extensions, LineBreaks, MarkdownOptions, MarkupError, PassthroughKind, RawHtml,
    TagStyle,
};

mod blocks;
mod hooks;
mod walk;

/// The hook kinds, for per-kind ordinals and errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookKind {
    Link,
    Image,
    Heading,
    CodeBlock,
    Blockquote,
    Table,
    Passthrough,
}

impl HookKind {
    const COUNT: usize = 7;

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Image => "image",
            Self::Heading => "heading",
            Self::CodeBlock => "code block",
            Self::Blockquote => "blockquote",
            Self::Table => "table",
            Self::Passthrough => "passthrough",
        }
    }
}

/// The page a render belongs to.
#[derive(Clone, Copy)]
pub(crate) struct Env<'r> {
    pub page: PageId,
    pub contexts: &'r SourceContexts,
    pub file: &'r Arc<Path>,
}

struct TableBuild {
    head: Vec<Vec<Cell>>,
    body: Vec<Vec<Cell>>,
    alignments: Vec<TableAlignment>,
}

enum Step<'a> {
    Enter(Node<'a>),
    Exit(Node<'a>, usize),
}

enum Walk {
    Children,
    Done,
}

pub(crate) struct Renderer<'r, 'a> {
    doc: &'r Doc<'a>,
    o: &'r MarkdownOptions,
    hooks: Option<&'r dyn Hooks>,
    hl: Option<&'r dyn Highlighter>,
    env: Env<'r>,
    /// Calls so far per [`HookKind`].
    ordinals: [u32; HookKind::COUNT],
    tables: Vec<TableBuild>,
    out: String,
}

static ALERT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^<p>\[!([a-zA-Z]+)\]([-+])?[\t\x0C ]?([^\n]*)\n?").expect("valid alert pattern")
});

impl<'r, 'a> Renderer<'r, 'a> {
    pub(crate) fn new(
        doc: &'r Doc<'a>,
        o: &'r MarkdownOptions,
        hooks: Option<&'r dyn Hooks>,
        hl: Option<&'r dyn Highlighter>,
        env: Env<'r>,
    ) -> Self {
        Self {
            doc,
            o,
            hooks,
            hl,
            env,
            ordinals: [0; HookKind::COUNT],
            tables: Vec::new(),
            out: String::new(),
        }
    }

    /// The whole document.
    pub(crate) fn document(mut self) -> Result<String, MarkupError> {
        self.walk(self.doc.root)?;
        Ok(self.out)
    }

    /// The HTML of `n`'s children.
    pub(crate) fn children(mut self, n: Node<'a>) -> Result<String, MarkupError> {
        for c in n.children() {
            self.walk(c)?;
        }
        Ok(self.out)
    }

    fn walk(&mut self, root: Node<'a>) -> Result<(), MarkupError> {
        let mut stack = vec![Step::Enter(root)];
        while let Some(step) = stack.pop() {
            match step {
                Step::Enter(n) => {
                    if let Walk::Children = self.enter(n)? {
                        stack.push(Step::Exit(n, self.out.len()));
                        stack.extend(n.reverse_children().map(Step::Enter));
                    }
                }
                Step::Exit(n, mark) => self.exit(n, mark)?,
            }
        }
        Ok(())
    }

    fn env(&mut self, kind: HookKind, n: Node<'_>) -> HookEnv {
        let offset = self.doc.original_start(n);
        let ord = &mut self.ordinals[kind as usize];
        let ordinal = *ord;
        *ord += 1;
        HookEnv {
            page: self.env.page,
            inner_page: self.env.contexts.inner_page(offset, self.env.page),
            ordinal,
            position: self.doc.src.position(self.env.file, offset),
        }
    }

    fn hook_error(&self, kind: HookKind, n: Node<'_>, source: HookError) -> MarkupError {
        let offset = self.doc.original_start(n);
        MarkupError::Hook {
            kind: kind.name(),
            position: self.doc.src.position(self.env.file, offset),
            source,
        }
    }

    /// A link or image URL; empty for a dangerous one unless raw HTML is allowed.
    fn href(&mut self, url: &str) {
        if self.o.raw_html == RawHtml::Omit && escape::dangerous_url(url) {
            return;
        }
        escape::url(&mut self.out, url);
    }

    fn void(&self) -> &'static str {
        match self.o.tags {
            TagStyle::Html => ">",
            TagStyle::Xhtml => " />",
        }
    }

    fn attrs_of(&self, n: Node<'_>) -> &'r [(String, ssg_base::Value)] {
        let doc: &'r Doc<'a> = self.doc;
        doc.extra(n).map_or(&[], |e| e.attrs.as_slice())
    }

    /// A paragraph written without `<p>`: goldmark's text blocks.
    fn text_block(&self, p: Node<'_>) -> bool {
        self.doc.text_block(p)
    }

    fn checkbox(&mut self, item: Node<'_>) {
        if let NodeValue::TaskItem(t) = &item.data().value {
            self.out.push_str(if t.symbol.is_some() {
                "<input checked=\"\" disabled=\"\" type=\"checkbox\""
            } else {
                "<input disabled=\"\" type=\"checkbox\""
            });
            self.out.push_str(self.void());
            self.out.push(' ');
        }
    }
}
