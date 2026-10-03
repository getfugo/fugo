//! Render jobs and their outputs (REWRITE_PLAN.md §2.6, §3.3). Frozen by T38.

use ssg_base::paths::OutputPath;
use ssg_base::{FormatId, LangIdx, PageId};
pub use ssg_nav::AliasPlan;

/// One unit of phase E work; [`Session::render_job`](crate::Session::render_job) turns it into
/// outputs without I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    /// A front matter alias (`ssg-nav`'s plan; its `kind` is `FrontMatter`).
    Alias(AliasPlan),
    /// A page in one format (pager 1 when the render paginates).
    Page { page: PageId, format: FormatId },
    /// Wave 2: pager `number` (≥ 2) of a recorded pagination.
    Pager {
        page: PageId,
        format: FormatId,
        number: u32,
    },
    /// Wave 2: `…/page/1/` → the page (HTML formats only).
    PagerAlias { page: PageId, format: FormatId },
    /// 404, sitemap, sitemap index, robots.txt.
    Standalone { page: PageId, format: FormatId },
    /// The default language's redirect (`ssg_nav::language_redirect`): `/<default
    /// language>/` → the site root, or `/` → `/<default language>/` when the default language
    /// is in a subdirectory.
    LanguageRedirect,
}

/// The order of jobs and outputs: language, format rank, tree key rank, then the job's sub
/// order. Collisions of targets are resolved by it (the later one wins).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobOrder {
    lang: LangIdx,
    format_rank: u8,
    key_rank: u32,
    sub: u8,
}

impl JobOrder {
    #[must_use]
    pub const fn new(lang: LangIdx, format_rank: u8, key_rank: u32, sub: u8) -> Self {
        Self {
            lang,
            format_rank,
            key_rank,
            sub,
        }
    }

    /// The language of the job.
    #[must_use]
    pub const fn lang(self) -> LangIdx {
        self.lang
    }
}

/// A rendered file, before publishing (`ssg-build` hands it to the publisher).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// The file under `publishDir`.
    pub path: OutputPath,
    pub text: String,
    pub format: FormatId,
    /// The language whose base URL canonifies the output.
    pub lang: LangIdx,
    pub is_html: bool,
    pub order: JobOrder,
}
