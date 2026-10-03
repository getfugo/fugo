//! Markdown through comrak behind an engine-neutral API, plus the Go implementation's passes:
//! heading anchors, TOC and fragments, summaries, word count, the [`Hooks`] and
//! [`Highlighter`] traits and source-context spans (REWRITE_PLAN.md §2.4).
//!
//! [`render`] parses the expanded Markdown of a page, runs the Go implementation's passes
//! (goldmark's pipe tables and definition lists, heading and definition-term ids, heading and
//! block attributes, the context markers of includes, passthrough, linkify, typographer,
//! dropped comments) and renders HTML the way the Go implementation's goldmark setup does,
//! calling the render hooks post-order. [`fragments`] is the parse-only variant for the
//! fragments memo stage. The crate `README.md` records the engine decision and the accepted
//! differences from the Go implementation.

#![forbid(unsafe_code)]

mod attributes;
mod doc;
mod escape;
mod hooks;
mod options;
mod parse;
mod passes;
mod render;
mod source;
pub mod text;
mod toc;

use comrak::Arena;
use comrak::nodes::NodeValue;
use ssg_base::diag::Position;

pub use hooks::{
    AlertSign, Alignment, BlockquoteCtx, BlockquoteKind, Cell, CodeBlockCtx, HeadingCtx,
    HighlightOptions, Highlighter, HookEnv, HookError, HookOut, Hooks, ImageCtx, LinkCtx, NoHooks,
    PassthroughCtx, PassthroughKind, TableCtx,
};
pub use options::{
    CodeFences, Delimiters, Extensions, LineBreaks, LinkifyProtocol, MarkdownOptions, RawHtml,
    StandaloneImages, TagStyle, TocOptions, Typographer,
};
pub use source::{
    CONTEXT_CLOSE, CONTEXT_OPEN, ExpandedMarkdown, SourceContexts, strip_context_markers,
    wrap_context,
};
pub use text::Summary;
pub use toc::{Fragments, Heading, Toc};

use crate::doc::Doc;
use crate::render::{Env, Renderer};

/// Why Markdown could not be rendered.
#[derive(Debug, thiserror::Error)]
pub enum MarkupError {
    /// A render hook or the highlighter failed.
    #[error("{position}: {kind} render hook: {source}")]
    Hook {
        kind: &'static str,
        position: Position,
        #[source]
        source: HookError,
    },
    /// Invalid `{…}` attributes (a fence's `{=x}`, a `null` value, a non-string id).
    #[error("{position}: {message}")]
    Attributes { position: Position, message: String },
}

/// Rendered Markdown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderedMarkdown {
    pub html: String,
    pub toc: Toc,
    pub fragments: Fragments,
}

/// The headings of `src` without rendering it (no hooks run).
///
/// # Errors
/// Invalid attributes.
pub fn fragments(
    src: &ExpandedMarkdown<'_>,
    o: &MarkdownOptions,
) -> Result<Fragments, MarkupError> {
    let arena = Arena::new();
    let doc = parse::parse(&arena, src, o)?;
    headings(&doc, o, src).map(|(_, f)| f)
}

/// Renders `src` with hooks `h`; fenced code no hook handles goes to `hl` (plain
/// `<pre><code>` without one).
///
/// # Errors
/// A failing hook or highlighter, or invalid attributes.
pub fn render(
    src: &ExpandedMarkdown<'_>,
    o: &MarkdownOptions,
    h: &dyn Hooks,
    hl: Option<&dyn Highlighter>,
) -> Result<RenderedMarkdown, MarkupError> {
    let arena = Arena::new();
    let doc = parse::parse(&arena, src, o)?;
    let html = Renderer::new(&doc, o, Some(h), hl, env(src)).document()?;
    let (toc, fragments) = headings(&doc, o, src)?;
    Ok(RenderedMarkdown {
        html,
        toc,
        fragments,
    })
}

fn env<'r>(src: &ExpandedMarkdown<'r>) -> Env<'r> {
    Env {
        page: src.page,
        contexts: src.contexts,
        file: src.file,
    }
}

/// The table of contents and fragments of a parsed document.
fn headings(
    doc: &Doc<'_>,
    o: &MarkdownOptions,
    src: &ExpandedMarkdown<'_>,
) -> Result<(Toc, Fragments), MarkupError> {
    let mut entries = Vec::new();
    let mut identifiers = Vec::new();
    for n in doc.root.descendants() {
        let h = match n.data().value {
            NodeValue::Heading(h) => h,
            NodeValue::DescriptionTerm => {
                identifiers.extend(doc.extra(n).and_then(|e| e.id.clone()));
                continue;
            }
            _ => continue,
        };
        let id = doc.extra(n).and_then(|e| e.id.clone());
        let html = Renderer::new(doc, o, None, None, env(src)).children(n)?;
        let heading = Heading {
            level: if id.is_some() { h.level } else { 0 },
            id: id.clone().unwrap_or_default(),
            html,
            plain: passes::ids::text_plain(doc, n),
            children: Vec::new(),
        };
        if let Some(id) = id {
            identifiers.push(id);
        }
        entries.push((h.level, heading));
    }
    identifiers.sort();
    let tree = toc::tree(entries);
    Ok((
        Toc {
            headings: tree.clone(),
        },
        Fragments {
            headings: tree,
            identifiers,
        },
    ))
}
