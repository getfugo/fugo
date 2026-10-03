//! The render session (REWRITE_PLAN.md §2.6, §3.2–3.3): the Tera instance with the template
//! functions, the content phase, the view freeze and the render jobs of phase E.
//!
//! **State: T34.** The content engine is real: shortcodes (the private `shortcode` and
//! `tokens` modules), summaries ([`summary`]), render hooks through Tera, memo cells that never
//! block with cycle detection through the scope's chain, fragments as their own stage,
//! page-store writes buffered per computation and committed by the winner, one content variant
//! per hook format.
//! Frozen by T38: [`Job`], [`JobOrder`], [`Output`] and
//! `Session::{new, render_content, freeze_views, render_job}`. The site functions are
//! `ssg_sitefuncs::register`'s (T35); `ssg-build` (T36) runs the phases, with the jobs
//! of [`Session::wave1`] / [`Session::wave2`], the targets of [`Session::target`] and the
//! deferred templates of [`Session::render_deferred`]. The lookup inputs [`lookup_path`],
//! [`layout_query`] and [`rendered_formats`] are public for `templates check` (T37),
//! so its coverage runs exactly the build's lookups.

#![forbid(unsafe_code)]

mod content;
mod hooks;
mod i18n;
mod job;
mod memo;
mod session;
mod shortcode;
pub mod summary;
mod tokens;

use ssg_base::{FormatId, PageId};

pub use job::{AliasPlan, Job, JobOrder, Output};
pub use session::{Project, RenderOptions, Session, layout_query, lookup_path, rendered_formats};

/// Why a render failed.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// A pager's file or link could not be made.
    #[error(transparent)]
    Target(Box<ssg_view::TargetError>),
    /// The views could not be built.
    #[error(transparent)]
    View(Box<ssg_view::ViewError>),
    /// The alias plan could not be made.
    #[error(transparent)]
    Nav(Box<ssg_nav::NavError>),
    /// The templates did not load.
    #[error(transparent)]
    Template(#[from] ssg_layouts::TemplateError),
    /// Markdown failed.
    #[error("{}: {message}", file.display())]
    Markup {
        file: std::path::PathBuf,
        message: String,
    },
    /// The content of a page failed (a shortcode, a hook, Markdown, a cycle).
    #[error("{page}: {source}")]
    Content {
        page: String,
        #[source]
        source: Box<ssg_view::ContentError>,
    },
    /// A template failed.
    #[error("{template} ({page}): {source}")]
    Render {
        template: String,
        page: String,
        #[source]
        source: Box<tera::Error>,
    },
    /// A content adapter (`_content.html`) failed.
    #[error("content adapter {path}: {source}")]
    Adapter {
        path: String,
        #[source]
        source: Box<tera::Error>,
    },
    /// A job for a format the page is not rendered in.
    #[error("page {page} has no output in format {format}")]
    NoOutput { page: PageId, format: FormatId },
    /// A phase called out of order.
    #[error("render phase out of order: {0}")]
    Phase(&'static str),
    /// An i18n file does not load.
    #[error(transparent)]
    I18n(Box<ssg_locale::I18nError>),
    /// A component directory cannot be read.
    #[error(transparent)]
    Vfs(Box<ssg_vfs::VfsError>),
    /// A file cannot be read.
    #[error("{}: {source}", path.display())]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A `defer(...)` template failed in phase E5.
    #[error("defer(key=\"{key}\") ({template}): {source}")]
    Deferred {
        key: String,
        template: String,
        #[source]
        source: Box<tera::Error>,
    },
}

impl From<ssg_vfs::VfsError> for RenderError {
    fn from(e: ssg_vfs::VfsError) -> Self {
        Self::Vfs(Box::new(e))
    }
}

impl From<ssg_view::TargetError> for RenderError {
    fn from(e: ssg_view::TargetError) -> Self {
        Self::Target(Box::new(e))
    }
}

impl From<ssg_view::ViewError> for RenderError {
    fn from(e: ssg_view::ViewError) -> Self {
        Self::View(Box::new(e))
    }
}

impl From<ssg_nav::NavError> for RenderError {
    fn from(e: ssg_nav::NavError) -> Self {
        Self::Nav(Box::new(e))
    }
}
