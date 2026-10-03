//! Which renderer a page's content goes through.

use ssg_config::MediaTypes;

use crate::PageError;

/// The content renderer of a page: comrak for Markdown, pass-through for HTML.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Markup {
    #[default]
    Markdown,
    Html,
}

/// Markup identifiers Go accepts besides media types and suffixes.
const MARKDOWN_NAMES: &[&str] = &["markdown", "goldmark", "md", "mdown"];
const HTML_NAMES: &[&str] = &["html", "htm"];
/// Content types Go renders with external tools; not supported here.
const EXTERNAL_NAMES: &[&str] = &[
    "asciidoc",
    "asciidocext",
    "adoc",
    "ad",
    "rst",
    "pandoc",
    "pdc",
    "org",
];

/// Where a page's markup is declared, strongest first: front matter `mediaType`, front matter
/// `markup`, the content file's extension.
#[derive(Clone, Copy, Debug, Default)]
pub struct MarkupSource<'a> {
    /// Front matter `mediaType` (`text/html`).
    pub media_type: Option<&'a str>,
    /// Front matter `markup` (`md`, `html`, `goldmark`, or a media type).
    pub markup: Option<&'a str>,
    /// The content file's extension without the dot; empty for pages without a file.
    pub ext: &'a str,
}

impl Markup {
    /// Detects the markup of a page. An empty source (no front matter keys, no file) is
    /// Markdown.
    ///
    /// # Errors
    /// [`PageError::UnknownMarkup`] for a value that names no content type,
    /// [`PageError::UnsupportedMarkup`] for AsciiDoc, reStructuredText, Pandoc and Org, and
    /// for media types other than Markdown and HTML.
    pub fn detect(src: MarkupSource<'_>, types: &MediaTypes) -> Result<Self, PageError> {
        if let Some(mt) = src.media_type.filter(|s| !s.is_empty()) {
            return Self::from_media_type(mt, types);
        }
        if let Some(name) = src.markup.filter(|s| !s.is_empty()) {
            return Self::from_name(name, types);
        }
        if src.ext.is_empty() {
            return Ok(Self::Markdown);
        }
        Self::from_suffix(src.ext, types)
    }

    /// The markup of a front matter `markup` value.
    ///
    /// # Errors
    /// See [`Markup::detect`].
    pub fn from_name(name: &str, types: &MediaTypes) -> Result<Self, PageError> {
        let lower = name.to_ascii_lowercase();
        if MARKDOWN_NAMES.contains(&lower.as_str()) {
            return Ok(Self::Markdown);
        }
        if HTML_NAMES.contains(&lower.as_str()) {
            return Ok(Self::Html);
        }
        if EXTERNAL_NAMES.contains(&lower.as_str()) {
            return Err(PageError::UnsupportedMarkup(name.to_owned()));
        }
        if lower.contains('/') {
            return Self::from_media_type(&lower, types);
        }
        Self::from_suffix(&lower, types)
    }

    fn from_media_type(mt: &str, types: &MediaTypes) -> Result<Self, PageError> {
        let id = types
            .by_type(mt)
            .ok_or_else(|| PageError::UnknownMarkup(mt.to_owned()))?;
        let t = types.get(id);
        if t.is_markdown() {
            Ok(Self::Markdown)
        } else if t.is_html() {
            Ok(Self::Html)
        } else {
            Err(PageError::UnsupportedMarkup(mt.to_owned()))
        }
    }

    fn from_suffix(suffix: &str, types: &MediaTypes) -> Result<Self, PageError> {
        let markdown = types.by_type("text/markdown").map(|id| types.get(id));
        let html = types.by_type("text/html").map(|id| types.get(id));
        if markdown.is_some_and(|t| t.has_suffix(suffix)) {
            Ok(Self::Markdown)
        } else if html.is_some_and(|t| t.has_suffix(suffix)) {
            Ok(Self::Html)
        } else if types.by_suffix(suffix).is_some() {
            Err(PageError::UnsupportedMarkup(suffix.to_owned()))
        } else {
            Err(PageError::UnknownMarkup(suffix.to_owned()))
        }
    }

    /// The name `.Markup` reports (`markdown`, `html`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Html => "html",
        }
    }
}
