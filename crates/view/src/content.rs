//! The one callback from site functions into the render engine (REWRITE_PLAN.md §2.5):
//! [`ContentRenderer`], and the values it returns.
//!
//! The trait's five methods and their signatures are frozen by T38; the fields of
//! [`RenderedContent`], [`ExpandedSource`] and [`RenderStringOptions`] are the skeleton's and
//! may grow in T34 (fields are only added).

use std::sync::Arc;

use ssg_base::PageId;
use ssg_layouts::TemplateName;
use ssg_markup::{Fragments, SourceContexts};

use crate::scope::{HookVariant, RenderScope};

/// A page's content in one hook variant, after shortcodes, Markdown and placeholder swaps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderedContent {
    /// `.Content` (HTML).
    pub html: String,
    /// `.Summary` (HTML): the text before `<!--more-->`, the front matter summary, or the
    /// automatic summary.
    pub summary: String,
    /// `.Truncated`.
    pub truncated: bool,
    /// `.Plain`.
    pub plain: String,
    pub word_count: usize,
    pub fuzzy_word_count: usize,
    /// Minutes (`.ReadingTime`).
    pub reading_time: usize,
    /// `.TableOfContents` (HTML).
    pub table_of_contents: String,
    pub fragments: Arc<Fragments>,
}

/// A page's Markdown after its shortcodes ran: `{{% %}}` output spliced in, `{{< >}}` output
/// behind page-local `NHSC<n>X` tokens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpandedSource {
    pub markdown: String,
    /// The output of each `{{< >}}` call, indexed by its token number.
    pub placeholders: Vec<Arc<str>>,
    /// Spans of `markdown` that came from other pages (`render_shortcodes` includes).
    pub contexts: SourceContexts,
}

/// Options of `markdownify` / `render_string`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStringOptions {
    /// Render as a block: keep the `<p>` of a single paragraph (`markdownify` strips it).
    pub display_block: bool,
}

/// Why content could not be rendered.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    /// A memo cell asked for itself through other pages.
    #[error("content cycle: {0}")]
    Cycle(String),
    /// Nesting deeper than the scope allows.
    #[error("render nesting deeper than {limit}")]
    TooDeep { limit: u16 },
    /// The renderer is gone (the session was dropped).
    #[error("the render session is gone")]
    NoSession,
    /// A page without content in this render.
    #[error("page {0} has no content")]
    NoContent(PageId),
    /// Markdown, a shortcode, a hook or a template failed.
    #[error("{0}")]
    Render(String),
}

/// The only callback from site functions into the render engine. Site functions hold a
/// `Weak<dyn ContentRenderer>`; every call passes the caller's scope, which carries the cycle
/// chain.
pub trait ContentRenderer: Send + Sync {
    /// Page `p`'s rendered content in variant `v`.
    ///
    /// # Errors
    /// See [`ContentError`].
    fn content(
        &self,
        p: PageId,
        v: HookVariant,
        s: &RenderScope,
    ) -> Result<Arc<RenderedContent>, ContentError>;

    /// Page `p`'s headings and identifiers (parse only, no hooks).
    ///
    /// # Errors
    /// See [`ContentError`].
    fn fragments(&self, p: PageId, s: &RenderScope) -> Result<Arc<Fragments>, ContentError>;

    /// Page `p`'s Markdown after its shortcodes ran (`render_shortcodes`).
    ///
    /// # Errors
    /// See [`ContentError`].
    fn render_shortcodes(
        &self,
        p: PageId,
        s: &RenderScope,
    ) -> Result<Arc<ExpandedSource>, ContentError>;

    /// Markdown text rendered in the scope's page (`markdownify`, `render_string`).
    ///
    /// # Errors
    /// See [`ContentError`].
    fn render_markdown(
        &self,
        md: &str,
        o: RenderStringOptions,
        s: &RenderScope,
    ) -> Result<String, ContentError>;

    /// A template rendered with `ctx` (plus `__nh`): `partial()`, `execute_as_template`.
    ///
    /// # Errors
    /// See [`ContentError`].
    fn render_template(
        &self,
        t: &TemplateName,
        ctx: tera::Context,
        s: &RenderScope,
    ) -> Result<String, ContentError>;
}
