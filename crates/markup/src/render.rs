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

    #[expect(clippy::too_many_lines, reason = "one arm per node kind")]
    fn enter(&mut self, n: Node<'a>) -> Result<Walk, MarkupError> {
        let value = n.data().value.clone();
        match value {
            NodeValue::Document
            | NodeValue::BlockQuote
            | NodeValue::Heading(_)
            | NodeValue::Link(_)
            | NodeValue::Image(_)
            | NodeValue::TableCell
            | NodeValue::TableRow(_)
            | NodeValue::DescriptionItem(_)
            | NodeValue::Escaped => {
                if let NodeValue::TableRow(header) = value
                    && let Some(t) = self.tables.last_mut()
                {
                    if header { &mut t.head } else { &mut t.body }.push(Vec::new());
                }
                Ok(Walk::Children)
            }
            NodeValue::FrontMatter(_) | NodeValue::FootnoteDefinition(_) => Ok(Walk::Done),
            NodeValue::List(l) => {
                self.out.push_str(match l.list_type {
                    ListType::Bullet => "<ul",
                    ListType::Ordered => "<ol",
                });
                if l.list_type == ListType::Ordered && l.start != 1 {
                    self.out.push_str(&format!(" start=\"{}\"", l.start));
                }
                let a = self.attrs_of(n);
                escape::attrs(&mut self.out, a, &["start", "reversed", "type"]);
                self.out.push_str(">\n");
                Ok(Walk::Children)
            }
            NodeValue::Item(_) | NodeValue::TaskItem(_) => {
                self.out.push_str("<li");
                let a = self.attrs_of(n);
                escape::attrs(&mut self.out, a, &["value"]);
                self.out.push('>');
                match n.first_child() {
                    Some(fc) if matches!(fc.data().value, NodeValue::Paragraph) => {
                        if !self.text_block(fc) {
                            self.out.push('\n');
                        }
                    }
                    Some(_) => {
                        self.out.push('\n');
                        self.checkbox(n);
                    }
                    None => self.checkbox(n),
                }
                Ok(Walk::Children)
            }
            NodeValue::DescriptionList => {
                self.out.push_str("<dl");
                let a = self.attrs_of(n);
                escape::attrs(&mut self.out, a, &[]);
                self.out.push_str(">\n");
                Ok(Walk::Children)
            }
            NodeValue::DescriptionTerm => {
                self.out.push_str("<dt");
                if let Some(id) = self.doc.extra(n).and_then(|e| e.id.as_deref()) {
                    self.out.push_str(" id=\"");
                    escape::html(&mut self.out, id);
                    self.out.push('"');
                }
                self.out.push('>');
                Ok(Walk::Children)
            }
            NodeValue::DescriptionDetails => {
                self.out.push_str("<dd>");
                if !matches!(self.doc.role(n), Some(Role::TightDetails)) {
                    self.out.push('\n');
                }
                Ok(Walk::Children)
            }
            NodeValue::Paragraph => {
                if !self.text_block(n) {
                    self.out.push_str("<p");
                    let a = self.attrs_of(n);
                    escape::attrs(&mut self.out, a, &[]);
                    self.out.push('>');
                }
                if let Some(item) = n.parent()
                    && item.first_child().is_some_and(|f| f.same_node(n))
                {
                    self.checkbox(item);
                }
                Ok(Walk::Children)
            }
            NodeValue::ThematicBreak => {
                self.out.push_str("<hr");
                let a = self.attrs_of(n);
                escape::attrs(&mut self.out, a, &[]);
                self.out.push_str(self.void());
                self.out.push('\n');
                Ok(Walk::Done)
            }
            NodeValue::CodeBlock(cb) => {
                self.code_block(n, &cb)?;
                Ok(Walk::Done)
            }
            NodeValue::HtmlBlock(b) => {
                match self.o.raw_html {
                    RawHtml::Pass => self.out.push_str(&b.literal),
                    RawHtml::Omit => self.out.push_str("<!-- raw HTML omitted -->\n"),
                }
                Ok(Walk::Done)
            }
            NodeValue::Table(t) => {
                self.tables.push(TableBuild {
                    head: Vec::new(),
                    body: Vec::new(),
                    alignments: t.alignments.clone(),
                });
                Ok(Walk::Children)
            }
            NodeValue::Text(t) => {
                escape::html(&mut self.out, &t);
                Ok(Walk::Done)
            }
            NodeValue::SoftBreak => {
                if self.o.line_breaks == LineBreaks::Hard {
                    self.out.push_str("<br");
                    self.out.push_str(self.void());
                }
                self.out.push('\n');
                Ok(Walk::Done)
            }
            NodeValue::LineBreak => {
                self.out.push_str("<br");
                self.out.push_str(self.void());
                self.out.push('\n');
                Ok(Walk::Done)
            }
            NodeValue::Code(c) => {
                self.out.push_str("<code>");
                escape::html(&mut self.out, &c.literal);
                self.out.push_str("</code>");
                Ok(Walk::Done)
            }
            NodeValue::HtmlInline(h) => {
                match self.o.raw_html {
                    RawHtml::Pass => self.out.push_str(&h),
                    RawHtml::Omit => self.out.push_str("<!-- raw HTML omitted -->"),
                }
                Ok(Walk::Done)
            }
            NodeValue::Raw(r) => {
                self.raw(n, &r)?;
                Ok(Walk::Done)
            }
            NodeValue::Emph => {
                self.out.push_str("<em>");
                Ok(Walk::Children)
            }
            NodeValue::Strong => {
                self.out.push_str("<strong>");
                Ok(Walk::Children)
            }
            NodeValue::Strikethrough => {
                self.out.push_str("<del>");
                Ok(Walk::Children)
            }
            NodeValue::FootnoteReference(f) => {
                let back = if f.ref_num > 1 {
                    (f.ref_num - 1).to_string()
                } else {
                    String::new()
                };
                self.out.push_str(&format!(
                    "<sup id=\"fnref{back}:{ix}\"><a href=\"#fn:{ix}\" class=\"footnote-ref\" role=\"doc-noteref\">{ix}</a></sup>",
                    ix = f.ix
                ));
                Ok(Walk::Done)
            }
            NodeValue::ShortCode(sc) => {
                // goldmark-emoji's `Entity` rendering (v1.0.6 `renderEmoji`): the zero-width
                // joiner by name.
                for c in sc.emoji.chars() {
                    if c == '\u{200d}' {
                        self.out.push_str("&zwj;");
                    } else {
                        self.out.push_str(&format!("&#x{:x};", u32::from(c)));
                    }
                }
                Ok(Walk::Done)
            }
            _ => Ok(Walk::Children),
        }
    }

    fn exit(&mut self, n: Node<'a>, mark: usize) -> Result<(), MarkupError> {
        let value = n.data().value.clone();
        match value {
            NodeValue::Document => self.footnotes()?,
            NodeValue::BlockQuote => self.blockquote(n, mark)?,
            NodeValue::List(l) => self.out.push_str(match l.list_type {
                ListType::Bullet => "</ul>\n",
                ListType::Ordered => "</ol>\n",
            }),
            NodeValue::Item(_) | NodeValue::TaskItem(_) => self.out.push_str("</li>\n"),
            NodeValue::DescriptionList => self.out.push_str("</dl>\n"),
            NodeValue::DescriptionTerm => self.out.push_str("</dt>\n"),
            NodeValue::DescriptionDetails => self.out.push_str("</dd>\n"),
            NodeValue::Paragraph => {
                if !self.text_block(n) {
                    self.out.push_str("</p>\n");
                } else if n.next_sibling().is_some()
                    && n.first_child().is_some()
                    && !n
                        .parent()
                        .is_some_and(|p| matches!(p.data().value, NodeValue::DescriptionTerm))
                {
                    self.out.push('\n');
                }
            }
            NodeValue::Heading(h) => self.heading(n, h.level, mark)?,
            NodeValue::Emph => self.out.push_str("</em>"),
            NodeValue::Strong => self.out.push_str("</strong>"),
            NodeValue::Strikethrough => self.out.push_str("</del>"),
            NodeValue::Link(l) => self.link(n, &l.url, &l.title, mark, false)?,
            NodeValue::Image(l) => self.link(n, &l.url, &l.title, mark, true)?,
            NodeValue::TableCell => {
                let text = self.out.split_off(mark);
                let col = n.preceding_siblings().count() - 1;
                let padded = matches!(self.doc.role(n), Some(Role::PaddedCell));
                if let Some(t) = self.tables.last_mut() {
                    let alignment = if padded {
                        Alignment::None
                    } else {
                        match t.alignments.get(col) {
                            Some(TableAlignment::Left) => Alignment::Left,
                            Some(TableAlignment::Center) => Alignment::Center,
                            Some(TableAlignment::Right) => Alignment::Right,
                            _ => Alignment::None,
                        }
                    };
                    let header = n
                        .parent()
                        .is_some_and(|r| matches!(r.data().value, NodeValue::TableRow(true)));
                    let rows = if header { &mut t.head } else { &mut t.body };
                    if let Some(row) = rows.last_mut() {
                        row.push(Cell { text, alignment });
                    }
                }
            }
            NodeValue::Table(_) => self.table(n)?,
            _ => {}
        }
        Ok(())
    }

    fn call(
        &mut self,
        kind: HookKind,
        n: Node<'_>,
        f: impl FnOnce(&dyn Hooks, &HookEnv) -> Result<HookOut, HookError>,
    ) -> Result<Option<String>, MarkupError> {
        let Some(h) = self.hooks else {
            return Ok(None);
        };
        let env = self.env(kind, n);
        match f(h, &env) {
            Ok(HookOut::Html(s)) => Ok(Some(s)),
            Ok(HookOut::Default) => Ok(None),
            Err(e) => Err(self.hook_error(kind, n, e)),
        }
    }

    fn heading(&mut self, n: Node<'a>, level: u8, mark: usize) -> Result<(), MarkupError> {
        let text = self.out.split_off(mark);
        let extra = self.doc.extra(n);
        let id = extra.and_then(|e| e.id.clone());
        let attrs = extra.map(|e| e.attrs.clone()).unwrap_or_default();
        if self.hooks.is_some() {
            let mut map = attributes::to_map(&attrs);
            if let Some(id) = &id {
                map.insert("id", ssg_base::Value::string(id));
            }
            let ctx = HeadingCtx {
                level,
                anchor: id.clone().unwrap_or_default(),
                text: text.clone(),
                plain_text: text_plain(self.doc, n),
                attributes: map,
            };
            if let Some(html) = self.call(HookKind::Heading, n, |h, env| h.heading(env, &ctx))? {
                self.out.push_str(&html);
                return Ok(());
            }
        }
        self.out.push_str(&format!("<h{level}"));
        if let Some(id) = &id {
            self.out.push_str(" id=\"");
            escape::html(&mut self.out, id);
            self.out.push('"');
        }
        escape::all_attrs(&mut self.out, &attrs);
        self.out.push('>');
        self.out.push_str(&text);
        self.out.push_str(&format!("</h{level}>\n"));
        Ok(())
    }

    fn link(
        &mut self,
        n: Node<'a>,
        url: &str,
        title: &str,
        mark: usize,
        image: bool,
    ) -> Result<(), MarkupError> {
        let inner = self.out.split_off(mark);
        let auto = match self.doc.role(n) {
            Some(Role::AutoLink { www }) => Some(*www),
            _ => None,
        };
        let url = match auto {
            Some(true) => format!("{}://{url}", self.o.linkify_protocol.as_str()),
            _ => url.to_owned(),
        };
        if self.hooks.is_some() {
            let (text, plain_text) = if auto.is_some() {
                let label: String = n.descendants().filter_map(crate::doc::text_of).collect();
                (label.clone(), label)
            } else {
                (inner.clone(), text_plain(self.doc, n))
            };
            // Hooks see the destination and title as written.
            let (destination, raw_title) = match auto {
                Some(_) => (url.clone(), title.to_owned()),
                None => self
                    .doc
                    .raw_link(n, image)
                    .unwrap_or_else(|| (url.clone(), title.to_owned())),
            };
            let ctx = LinkCtx {
                destination,
                title: raw_title,
                text,
                plain_text,
                is_block: matches!(self.doc.role(n), Some(Role::BlockImage)),
                attributes: attributes::to_map(self.attrs_of(n)),
            };
            let out = if image {
                self.call(HookKind::Image, n, |h, env| h.image(env, &ctx))?
            } else {
                self.call(HookKind::Link, n, |h, env| h.link(env, &ctx))?
            };
            if let Some(html) = out {
                self.out.push_str(&html);
                return Ok(());
            }
        }
        if image {
            self.out.push_str("<img src=\"");
            self.href(&url);
            self.out.push_str("\" alt=\"");
            self.out.push_str(&crate::text::strip_html(&inner));
            self.out.push('"');
            if !title.is_empty() {
                self.out.push_str(" title=\"");
                escape::html(&mut self.out, title);
                self.out.push('"');
            }
            let a = self.attrs_of(n);
            escape::attrs(
                &mut self.out,
                a,
                &["align", "height", "width", "loading", "decoding"],
            );
            self.out.push_str(self.void());
        } else {
            self.out.push_str("<a href=\"");
            self.href(&url);
            self.out.push('"');
            if !title.is_empty() {
                self.out.push_str(" title=\"");
                escape::html(&mut self.out, title);
                self.out.push('"');
            }
            self.out.push('>');
            self.out.push_str(&inner);
            self.out.push_str("</a>");
        }
        Ok(())
    }

    fn blockquote(&mut self, n: Node<'a>, mark: usize) -> Result<(), MarkupError> {
        let captured = self.out.split_off(mark);
        let text = captured.trim().to_owned();
        let attrs = self.attrs_of(n).to_vec();
        if self.hooks.is_some() {
            let alert = self
                .o
                .extensions
                .contains(Extensions::ALERTS)
                .then(|| ALERT.captures(&text))
                .flatten()
                .map(|m| {
                    let title = m.get(3).map_or("", |t| t.as_str()).trim();
                    let sign = match m.get(2).map(|s| s.as_str()) {
                        Some("+") => AlertSign::Plus,
                        Some("-") => AlertSign::Minus,
                        _ => AlertSign::None,
                    };
                    // The content without the `[!TYPE]` line; when that line closed its
                    // paragraph the rest starts with the next block.
                    let rest = &text[m.get(0).map_or(0, |m| m.end())..];
                    let (title, rest) = match title.strip_suffix("</p>") {
                        Some(t) => (t, rest.to_owned()),
                        None => (title, format!("<p>{rest}")),
                    };
                    (m[1].to_lowercase(), title.trim().to_owned(), sign, rest)
                });
            let ctx = match alert {
                Some((alert_type, alert_title, alert_sign, rest)) => BlockquoteCtx {
                    kind: BlockquoteKind::Alert,
                    alert_type,
                    alert_title,
                    alert_sign,
                    text: rest,
                    attributes: attributes::to_map(&attrs),
                },
                None => BlockquoteCtx {
                    kind: BlockquoteKind::Regular,
                    alert_type: String::new(),
                    alert_title: String::new(),
                    alert_sign: AlertSign::None,
                    text: text.clone(),
                    attributes: attributes::to_map(&attrs),
                },
            };
            if let Some(html) =
                self.call(HookKind::Blockquote, n, |h, env| h.blockquote(env, &ctx))?
            {
                self.out.push_str(&html);
                return Ok(());
            }
        }
        if attrs.is_empty() {
            self.out.push_str("<blockquote>\n");
        } else {
            self.out.push_str("<blockquote");
            escape::attrs(&mut self.out, &attrs, &["cite"]);
            self.out.push('>');
        }
        self.out.push_str(&text);
        self.out.push_str("</blockquote>\n");
        Ok(())
    }

    fn table(&mut self, n: Node<'a>) -> Result<(), MarkupError> {
        let Some(t) = self.tables.pop() else {
            return Ok(());
        };
        let attrs = self.attrs_of(n).to_vec();
        let ctx = TableCtx {
            thead: t.head,
            tbody: t.body,
            attributes: attributes::to_map(&attrs),
        };
        if let Some(html) = self.call(HookKind::Table, n, |h, env| h.table(env, &ctx))? {
            self.out.push_str(&html);
            return Ok(());
        }
        // Go's embedded table template: `range $k, $v := .Attributes` (key order), falsy
        // values skipped, `printf " %s=%q" $k ($v | transform.HTMLEscape)`.
        self.out.push_str("<table");
        for (k, v) in ctx.attributes.iter() {
            let falsy = match v {
                ssg_base::Value::Null => true,
                ssg_base::Value::Bool(b) => !b,
                ssg_base::Value::Int(i) => *i == 0,
                ssg_base::Value::Float(f) => *f == 0.0,
                ssg_base::Value::String(s) => s.is_empty(),
                _ => false,
            };
            if falsy {
                continue;
            }
            self.out.push_str(&format!(" {k}=\""));
            let mut text = String::new();
            escape::html(&mut text, &escape::value_text(v));
            // Go's html.EscapeString also escapes `'`, and writes `"` as `&#34;`.
            self.out
                .push_str(&text.replace('\'', "&#39;").replace("&quot;", "&#34;"));
            self.out.push('"');
        }
        self.out.push_str(">\n  <thead>");
        write_rows(&mut self.out, &ctx.thead, "th");
        self.out.push_str("\n  </thead>\n  <tbody>");
        write_rows(&mut self.out, &ctx.tbody, "td");
        self.out.push_str("\n  </tbody>\n</table>\n");
        Ok(())
    }

    fn code_block(
        &mut self,
        n: Node<'a>,
        cb: &comrak::nodes::NodeCodeBlock,
    ) -> Result<(), MarkupError> {
        if !cb.fenced {
            self.out.push_str("<pre><code>");
            escape::html(&mut self.out, &cb.literal);
            self.out.push_str("</code></pre>\n");
            return Ok(());
        }
        let info = cb.info.trim();
        let word = info.split(' ').next().unwrap_or("");
        if self.o.code_fences == CodeFences::Plain || self.hooks.is_none() {
            self.plain_code(word, &cb.literal);
            return Ok(());
        }
        let lang = word.split('{').next().unwrap_or("").to_owned();
        let (attrs, options) =
            fence_attributes(info).map_err(|message| MarkupError::Attributes {
                position: self.doc.position(n),
                message,
            })?;
        let inner = cb.literal.trim_end_matches(['\n', '\r']).to_owned();
        let ctx = CodeBlockCtx {
            lang: lang.clone(),
            inner: inner.clone(),
            options: attributes::to_map(&options),
            attributes: attributes::to_map(&attrs),
        };
        // `call` numbers the code block (hooks are set here).
        let ordinal = self.ordinals[HookKind::CodeBlock as usize];
        if let Some(html) = self.call(HookKind::CodeBlock, n, |h, env| h.code_block(env, &ctx))? {
            self.out.push_str(&html);
            return Ok(());
        }
        if let Some(hl) = self.hl {
            let o = HighlightOptions {
                options: ctx.options,
                attributes: ctx.attributes,
                ordinal,
            };
            let html = hl
                .highlight(&inner, &lang, &o)
                .map_err(|e| self.hook_error(HookKind::CodeBlock, n, e))?;
            self.out.push_str(&html);
            return Ok(());
        }
        self.plain_code(&lang, &cb.literal);
        Ok(())
    }

    /// goldmark's fenced code: `<pre><code class="language-x">`.
    fn plain_code(&mut self, lang: &str, code: &str) {
        self.out.push_str("<pre><code");
        if !lang.is_empty() {
            self.out.push_str(" class=\"language-");
            escape::html(&mut self.out, lang);
            self.out.push('"');
        }
        self.out.push('>');
        escape::html(&mut self.out, code);
        self.out.push_str("</code></pre>\n");
    }

    fn raw(&mut self, n: Node<'a>, literal: &str) -> Result<(), MarkupError> {
        match self.doc.role(n).cloned() {
            Some(Role::Passthrough { kind, inner, raw }) => {
                let ctx = PassthroughCtx {
                    kind,
                    inner,
                    attributes: Map::new(),
                };
                let hooked =
                    self.call(HookKind::Passthrough, n, |h, env| h.passthrough(env, &ctx))?;
                let block_level = n
                    .parent()
                    .is_some_and(|p| !p.data().value.contains_inlines());
                // Go (`markup/goldmark/passthrough`, `renderPassthroughBlock`) writes a hook's
                // output as it is: the next block follows without a newline. Without a hook
                // the source of a block ends its line.
                let newline = kind == PassthroughKind::Block && block_level && hooked.is_none();
                self.out.push_str(&hooked.unwrap_or(raw));
                if newline {
                    self.out.push('\n');
                }
            }
            _ => self.out.push_str(literal),
        }
        Ok(())
    }

    fn footnotes(&mut self) -> Result<(), MarkupError> {
        let mut index: HashMap<String, u32> = HashMap::new();
        for d in self.doc.root.descendants() {
            if let NodeValue::FootnoteReference(f) = &d.data().value {
                index.entry(f.name.clone()).or_insert(f.ix);
            }
        }
        let mut defs: Vec<(u32, u32, Node<'a>)> = self
            .doc
            .root
            .descendants()
            .filter_map(|d| match &d.data().value {
                NodeValue::FootnoteDefinition(f) => {
                    index.get(&f.name).map(|&ix| (ix, f.total_references, d))
                }
                _ => None,
            })
            .collect();
        if defs.is_empty() {
            return Ok(());
        }
        defs.sort_by_key(|(ix, ..)| *ix);
        self.out
            .push_str("<div class=\"footnotes\" role=\"doc-endnotes\">\n<hr");
        self.out.push_str(self.void());
        self.out.push_str("\n<ol>\n");
        let backlink = self
            .o
            .footnote_backlink
            .clone()
            .unwrap_or_else(|| "&#x21a9;&#xfe0e;".to_owned());
        for (ix, refs, def) in defs {
            self.out.push_str(&format!("<li id=\"fn:{ix}\">\n"));
            let start = self.out.len();
            for c in def.children() {
                self.walk(c)?;
            }
            let mut links = String::new();
            for k in 0..refs.max(1) {
                let k = if k == 0 { String::new() } else { k.to_string() };
                links.push_str(&format!(
                    "&#160;<a href=\"#fnref{k}:{ix}\" class=\"footnote-backref\" role=\"doc-backlink\">{backlink}</a>"
                ));
            }
            if self.out[start..].ends_with("</p>\n") {
                let at = self.out.len() - "</p>\n".len();
                self.out.insert_str(at, &links);
            } else {
                self.out.push_str(&links);
            }
            self.out.push_str("</li>\n");
        }
        self.out.push_str("</ol>\n</div>\n");
        Ok(())
    }
}

fn write_rows(out: &mut String, rows: &[Vec<Cell>], tag: &str) {
    for row in rows {
        out.push_str("\n      <tr>");
        for c in row {
            out.push_str(&format!("\n          <{tag}"));
            if c.alignment != Alignment::None {
                out.push_str(&format!(" style=\"text-align: {}\"", c.alignment.as_str()));
            }
            out.push('>');
            out.push_str(&c.text);
            out.push_str(&format!("</{tag}>"));
        }
        out.push_str("\n      </tr>");
    }
}

/// The `{…}` of a fence info string: `(attributes, options)`. A `{…}` that does not parse
/// is an error (a `{` without `}` is not an attribute block).
fn fence_attributes(info: &str) -> Result<attributes::Converted, String> {
    let Some(close) = info.find('}') else {
        return Ok(Default::default());
    };
    let Some(open) = info[..close].rfind('{') else {
        return Ok(Default::default());
    };
    let attrs = attributes::parse(&info[open..=close])
        .filter(|(_, n)| *n == close + 1 - open)
        .map(|(a, _)| a)
        .ok_or_else(|| {
            "failed to parse Markdown attributes; you may need to quote the values".to_owned()
        })?;
    attributes::convert(&attrs, Owner::CodeBlock).map_err(|e| e.to_string())
}
