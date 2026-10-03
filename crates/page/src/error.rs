//! The errors of the per-page rules.

use ssg_base::glob::GlobError;
use ssg_base::url::UrlError;

/// A page whose front matter, cascade, permalink pattern or URL cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum PageError {
    /// A front matter field has a value of the wrong shape.
    #[error("front matter field {key:?}: {message}")]
    Field { key: String, message: String },
    /// Front matter `url` with a scheme (`https://…`): pages are always on the site's host.
    #[error("front matter url {0:?}: absolute URLs are not supported, use a path")]
    AbsoluteUrl(String),
    /// Front matter `kind` that is not a content kind.
    #[error("front matter kind {0:?} is not one of home, section, page, taxonomy, term")]
    Kind(String),
    /// Front matter `outputs` naming a format the site does not define.
    #[error("unknown output format {0:?}")]
    OutputFormat(String),
    /// A `markup`, `mediaType` or file extension that is not a content type.
    #[error("{0:?} is not a content type")]
    UnknownMarkup(String),
    /// A content type other than Markdown and HTML (AsciiDoc, reStructuredText, Pandoc, Org).
    #[error("content type {0:?} is not supported (only Markdown and HTML are)")]
    UnsupportedMarkup(String),
    /// A permalink pattern names an attribute that is neither a known token nor a Go date
    /// layout.
    #[error("permalink pattern {pattern:?}: unknown attribute \":{attribute}\"")]
    PermalinkAttribute { pattern: String, attribute: String },
    /// A `:sections[a:b]` slice whose start is after its end.
    #[error("permalink pattern {pattern:?}: the section slice {slice:?} ends before it starts")]
    SectionSlice { pattern: String, slice: String },
    /// A cascade entry that cannot be decoded.
    #[error("cascade: {0}")]
    Cascade(String),
    /// A cascade target glob that does not compile.
    #[error("cascade target {field}: {source}")]
    CascadeGlob {
        field: &'static str,
        #[source]
        source: GlobError,
    },
    /// A cascade target `kind` glob that matches no page kind.
    #[error("cascade target kind {0:?} matches no page kind")]
    CascadeKind(String),
    /// An `add_page` map of a content adapter that cannot place a page.
    #[error("{0}")]
    Adapter(String),
    /// A link that is not a URL reference (a `url` with a broken `%` escape).
    #[error("page link {link:?}: {source}")]
    Link {
        link: String,
        #[source]
        source: UrlError,
    },
}

impl PageError {
    pub(crate) fn field(key: &str, message: impl Into<String>) -> Self {
        Self::Field {
            key: key.to_owned(),
            message: message.into(),
        }
    }
}
