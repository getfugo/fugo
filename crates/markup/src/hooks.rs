//! Render hooks: the [`Hooks`] trait, the context each hook receives, and the
//! [`Highlighter`] seam for fenced code no hook handles.

use serde::Serialize;
use ssg_base::diag::Position;
use ssg_base::{Map, PageId};

/// A hook or highlighter failure, reported by [`crate::render`] with the node's position.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct HookError(Box<dyn std::error::Error + Send + Sync>);

impl HookError {
    /// Wraps any error.
    pub fn new(e: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self(e.into())
    }
}

/// Where a hook runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookEnv {
    /// The page being rendered.
    pub page: PageId,
    /// The page whose source the node came from (Go's `.PageInner`): the innermost
    /// [`crate::SourceContexts`] span containing the node, else [`HookEnv::page`].
    pub inner_page: PageId,
    /// Per hook kind, in call order (0-based).
    pub ordinal: u32,
    /// The node's position in the expanded source.
    pub position: Position,
}

/// What a hook produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookOut {
    /// No hook for this node: render it the default way.
    Default,
    /// The hook's HTML, which replaces the node.
    Html(String),
}

/// Render hooks. Every method defaults to [`HookOut::Default`].
///
/// Hooks run post-order: nested hooks first, so a context's `text` is the rendered HTML of the
/// node's content including nested hook output; then the node is replaced by the hook's HTML.
pub trait Hooks: Sync {
    /// A link, including autolinks and linkify results.
    ///
    /// # Errors
    /// The hook's failure; rendering stops.
    fn link(&self, _: &HookEnv, _: &LinkCtx) -> Result<HookOut, HookError> {
        Ok(HookOut::Default)
    }
    /// An image.
    ///
    /// # Errors
    /// The hook's failure; rendering stops.
    fn image(&self, _: &HookEnv, _: &ImageCtx) -> Result<HookOut, HookError> {
        Ok(HookOut::Default)
    }
    /// A heading.
    ///
    /// # Errors
    /// The hook's failure; rendering stops.
    fn heading(&self, _: &HookEnv, _: &HeadingCtx) -> Result<HookOut, HookError> {
        Ok(HookOut::Default)
    }
    /// A fenced code block (only under [`crate::CodeFences::Hooked`]).
    ///
    /// # Errors
    /// The hook's failure; rendering stops.
    fn code_block(&self, _: &HookEnv, _: &CodeBlockCtx) -> Result<HookOut, HookError> {
        Ok(HookOut::Default)
    }
    /// A blockquote, regular or alert.
    ///
    /// # Errors
    /// The hook's failure; rendering stops.
    fn blockquote(&self, _: &HookEnv, _: &BlockquoteCtx) -> Result<HookOut, HookError> {
        Ok(HookOut::Default)
    }
    /// A table. [`HookOut::Default`] renders the Go implementation's embedded table template.
    ///
    /// # Errors
    /// The hook's failure; rendering stops.
    fn table(&self, _: &HookEnv, _: &TableCtx) -> Result<HookOut, HookError> {
        Ok(HookOut::Default)
    }
    /// A passthrough (math) element.
    ///
    /// # Errors
    /// The hook's failure; rendering stops.
    fn passthrough(&self, _: &HookEnv, _: &PassthroughCtx) -> Result<HookOut, HookError> {
        Ok(HookOut::Default)
    }
}

/// No hooks: every node renders the default way.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHooks;

impl Hooks for NoHooks {}

/// A link or image.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LinkCtx {
    pub destination: String,
    pub title: String,
    /// The rendered content (HTML).
    pub text: String,
    pub plain_text: String,
    /// An image that replaced its paragraph ([`crate::StandaloneImages::Block`]).
    pub is_block: bool,
    pub attributes: Map,
}

/// An image (the same fields as a link).
pub type ImageCtx = LinkCtx;

/// A heading.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HeadingCtx {
    pub level: u8,
    /// The id; empty without automatic or explicit id.
    pub anchor: String,
    pub text: String,
    pub plain_text: String,
    pub attributes: Map,
}

/// A fenced code block.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CodeBlockCtx {
    /// The language: the info string's first word, up to a `{`.
    #[serde(rename = "type")]
    pub lang: String,
    /// The code, trailing newlines removed.
    pub inner: String,
    /// Highlighter options from the info string's `{…}` (`linenos`, `hl_lines` as 0-based
    /// `[from, to]` ranges, `style`, …), with their key as written.
    pub options: Map,
    /// The other `{…}` attributes, keys lower-cased.
    pub attributes: Map,
}

/// Whether a blockquote is an alert.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockquoteKind {
    Regular,
    Alert,
}

/// The `+`/`-` after an alert type (foldable callouts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSign {
    None,
    Plus,
    Minus,
}

impl AlertSign {
    /// `+`, `-` or empty.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Plus => "+",
            Self::Minus => "-",
        }
    }
}

/// A blockquote.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BlockquoteCtx {
    #[serde(rename = "type")]
    pub kind: BlockquoteKind,
    /// Lower-cased (`note`); empty for a regular blockquote.
    pub alert_type: String,
    pub alert_title: String,
    pub alert_sign: AlertSign,
    /// The rendered content; for an alert without its `[!TYPE]` line.
    pub text: String,
    pub attributes: Map,
}

/// A column alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

impl Alignment {
    /// `left`, `center`, `right` or empty.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

/// A table cell.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Cell {
    /// The rendered content.
    pub text: String,
    /// The column's alignment; [`Alignment::None`] for cells padded into a short row.
    pub alignment: Alignment,
}

/// A table.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TableCtx {
    pub thead: Vec<Vec<Cell>>,
    pub tbody: Vec<Vec<Cell>>,
    pub attributes: Map,
}

/// Inline or block passthrough.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PassthroughKind {
    Inline,
    Block,
}

/// A passthrough (math) element.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PassthroughCtx {
    #[serde(rename = "type")]
    pub kind: PassthroughKind,
    /// The content between the delimiters.
    pub inner: String,
    pub attributes: Map,
}

/// What a highlighter needs besides the code and its language.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HighlightOptions {
    /// The fence's highlighter options (see [`CodeBlockCtx::options`]).
    pub options: Map,
    /// The fence's other attributes.
    pub attributes: Map,
    /// The fence's ordinal among the page's code blocks (from 0; [`HookEnv::ordinal`]).
    pub ordinal: u32,
}

/// Highlights fenced code that no code-block hook handled (implemented by `ssg-highlight`).
pub trait Highlighter: Send + Sync {
    /// The HTML of `code` in language `lang`.
    ///
    /// # Errors
    /// The highlighter's failure; rendering stops.
    fn highlight(&self, code: &str, lang: &str, o: &HighlightOptions) -> Result<String, HookError>;
}
