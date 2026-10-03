//! Template names and what a layout file is.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use ssg_base::PageKind;

/// A Tera template name. Only this crate constructs names: a user template's is its path in the
/// layouts component (`docs/list.html`), a theme's is prefixed with `_theme<N>/` and an
/// embedded one's with `_embedded/`; synthesised base variants are `<layout>@@<base>`, and a
/// template whose output format escapes differently from its file suffix gets an alias with
/// `@@plain` or `@@escaped.html` appended (§4.5).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TemplateName(Arc<str>);

impl TemplateName {
    pub(crate) fn new(s: impl Into<Arc<str>>) -> Self {
        Self(s.into())
    }

    /// The name as Tera knows it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TemplateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for TemplateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl AsRef<str> for TemplateName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// The Markdown element a render hook renders (`_markup/render-<kind>[-<variant>]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HookKind {
    Link,
    Image,
    Heading,
    CodeBlock,
    Blockquote,
    Table,
    Passthrough,
}

impl HookKind {
    /// Every hook kind.
    pub const ALL: [Self; 7] = [
        Self::Link,
        Self::Image,
        Self::Heading,
        Self::CodeBlock,
        Self::Blockquote,
        Self::Table,
        Self::Passthrough,
    ];

    /// The name in file names (`codeblock`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Image => "image",
            Self::Heading => "heading",
            Self::CodeBlock => "codeblock",
            Self::Blockquote => "blockquote",
            Self::Table => "table",
            Self::Passthrough => "passthrough",
        }
    }

    /// The hook kind named `s` (lower case).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

impl fmt::Display for HookKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The pages rendered in isolation, each by its own root template.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StandaloneKind {
    /// `404.html`.
    NotFound,
    /// `sitemap.xml`.
    Sitemap,
    /// `sitemapindex.xml`.
    SitemapIndex,
    /// `robots.txt`.
    RobotsTxt,
    /// `alias.html`.
    Alias,
}

impl StandaloneKind {
    /// Every standalone kind.
    pub const ALL: [Self; 5] = [
        Self::NotFound,
        Self::Sitemap,
        Self::SitemapIndex,
        Self::RobotsTxt,
        Self::Alias,
    ];

    /// The output format that renders it (`404`, `sitemap`, `robots`, `alias`).
    #[must_use]
    pub const fn format_name(self) -> &'static str {
        match self {
            Self::NotFound => "404",
            Self::Sitemap => "sitemap",
            Self::SitemapIndex => "sitemapindex",
            Self::RobotsTxt => "robots",
            Self::Alias => "alias",
        }
    }

    /// The page kind, for those that are pages.
    #[must_use]
    pub const fn page_kind(self) -> Option<PageKind> {
        match self {
            Self::NotFound => Some(PageKind::NotFound),
            Self::Sitemap => Some(PageKind::Sitemap),
            Self::SitemapIndex => Some(PageKind::SitemapIndex),
            Self::RobotsTxt => Some(PageKind::RobotsTxt),
            Self::Alias => None,
        }
    }
}

/// What a layout file is, resolved at scan time from its v0.146 name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateRole {
    /// A page or list template (`single.html`, `posts/list.rss.xml`, `home.html`).
    Layout {
        kind: Option<PageKind>,
        layout: Option<String>,
    },
    /// A base template (`baseof.html`, `docs/baseof.list.html`).
    Base {
        kind: Option<PageKind>,
        layout: Option<String>,
    },
    /// `_partials/<name>`: the name without identifiers (`helpers/picture`).
    Partial { name: String },
    /// `[<dir>/]_shortcodes/<name>`.
    Shortcode { name: String },
    /// `[<dir>/]_markup/render-<kind>[-<variant>]`.
    Hook {
        kind: HookKind,
        variant: Option<String>,
    },
    /// A root template of an isolated page (`404.html`, `sitemap.xml`, `robots.txt`,
    /// `alias.html`, `sitemapindex.xml`).
    Standalone(StandaloneKind),
}

/// Where a template comes from. User templates beat theme templates of the same path, which
/// beat embedded ones.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Origin {
    /// The project's layouts (the file's absolute path).
    User(PathBuf),
    /// The n-th theme (1-based, as in `_theme<n>/`) and the file's absolute path.
    Theme(u8, PathBuf),
    /// The Go implementation's embedded templates rewritten in Tera (`crates/layouts/embedded/`).
    Embedded,
}

impl Origin {
    /// The Tera name prefix of this origin (`""`, `_theme1/`, `_embedded/`).
    #[must_use]
    pub fn prefix(&self) -> String {
        match self {
            Self::User(_) => String::new(),
            Self::Theme(n, _) => format!("_theme{n}/"),
            Self::Embedded => crate::EMBEDDED_PREFIX.to_owned(),
        }
    }

    /// Precedence: 0 for the project, then the themes in order, embedded last.
    pub(crate) fn rank(&self) -> u16 {
        match self {
            Self::User(_) => 0,
            Self::Theme(n, _) => u16::from(*n),
            Self::Embedded => u16::MAX,
        }
    }

    pub(crate) fn is_embedded(&self) -> bool {
        matches!(self, Self::Embedded)
    }

    /// The file, for user and theme templates.
    #[must_use]
    pub fn file(&self) -> Option<&std::path::Path> {
        match self {
            Self::User(p) | Self::Theme(_, p) => Some(p),
            Self::Embedded => None,
        }
    }
}
