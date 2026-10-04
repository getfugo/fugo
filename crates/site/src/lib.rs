//! Capture and assembly into the [`Model`] (docs/rust-port/REWRITE_PLAN.md §2.4, phases A4–B5).
//!
//! [`load_model`] reads the content and data of a project and builds the page arena:
//!
//! 1. **Capture** (A4, parallel over files): discovery through the `Vfs`, front matter split and
//!    decoded into folded [`Params`], the capture overrides `kind`, `lang` and `path`, the page's
//!    own `cascade`. `data::load` builds `.Site.Data` alongside. Content adapters
//!    (`_content.html`) are listed, not run: [`capture_content`] returns the [`Captured`]
//!    content, the caller runs the adapters (ssg-build renders them with a model of the
//!    files alone) and [`assemble`] builds the model with the pages and resources they
//!    [`Added`]. [`load_model`] is both steps without adapters.
//! 2. **Tree** (B1): every page gets its language, key and kind (home, section, taxonomy, term
//!    or page) and enters its language's [`SiteTree`]; content files inside leaf bundles are
//!    [`PageRole::Bundled`] pages, other bundle files [`BundleResource`]s. Keys claimed twice keep
//!    the first file, with a warning.
//! 3. **Cascade → meta → dates → filter** (B2): the cascade handed down each tree
//!    ([`CascadeIndex`]), then [`ssg_page::meta_from_params`] with the language's date
//!    sources and time zone (parallel over pages), then drafts, future and expired content
//!    against the build clock.
//! 4. **Nodes** (B3, `nodes`): the pages Go makes itself: missing taxonomy pages, root
//!    sections and home page, standalone pages (404, sitemap, sitemap index, robots.txt), and
//!    term pages with their members (`taxonomy`).
//! 5. **Relations** (B5, `relations`): titles, sections and types; parents, sections and the
//!    default-sorted lists; node dates; translations (`translations`).
//! 6. **URLs** (B4, `urls`, parallel over pages): output formats, target paths and links; then
//!    bundle resources get their owner, name and target (`resources`).
//!
//! [`Model::get_page`] and [`Model::ref_link`] (`refs`) resolve page references.

#![forbid(unsafe_code)]

mod assembly;
mod capture;
mod cascade;
pub mod data;
mod filter;
pub mod meta;
mod nodes;
mod refs;
mod relations;
mod resources;
mod taxonomy;
mod translations;
mod tree;
mod urls;

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use jiff::Zoned;
use ssg_base::diag::Diagnostic;
use ssg_base::paths::{self, ContentKey};
use ssg_base::{
    Clock, FormatId, IdVec, LangIdx, Map, OutputPath, PageId, PageKind, Params, ResourceId,
    TaxonomyIdx, TermIdx, UrlPath,
};
use ssg_config::{Config, ContentFilter};
use ssg_page::{
    AdapterPage, Dates, Links, ListMode, PageError, PageMeta, PermalinkPatterns, RenderMode,
    ResourceBase, TargetPaths,
};
use ssg_vfs::{FileRef, PathInfo, PathParser, Vfs, VfsError};

pub use assembly::*;
pub use capture::{ContentAdapter, SourceFile};
pub use cascade::CascadeIndex;
pub use data::{Data, DataError};
pub use refs::{RefArgs, RefError, RefLink};
pub use taxonomy::{Taxonomy, Term, WeightedPage};
pub use tree::{PageRole, SiteTree};

use filter::Verdict;

/// A page of the model: a content file, or a page Go makes itself (a missing home page, root
/// section or taxonomy page, a term page, a standalone page such as `404`).
#[derive(Clone, Debug)]
pub struct Page {
    pub id: PageId,
    pub lang: LangIdx,
    pub kind: PageKind,
    pub role: PageRole,
    /// The key in the language's tree (`.Path` without the leading slash, except for standalone
    /// pages: `_robots` is `/_robots.txt`); a bundled page's key keeps its file extension
    /// (`post/notes.md`).
    pub key: ContentKey,
    /// `None` for pages without a content file.
    pub source: Option<SourceFile>,
    /// The page's path: its content file's (after front matter `path`), or the path Go gives
    /// a page it makes (`/tags/Blue Sky/_index.md`, `/404.html`). Names, sections, titles and
    /// URLs are read from it.
    pub path_info: PathInfo,
    /// Typed front matter after the cascade, with the dates resolved (a node without dates
    /// takes its descendants') and, for switched-off nodes, the build policy turned off.
    pub meta: PageMeta,
    /// `.Title`: front matter, else (pages without a file) the default title of the kind.
    pub title: String,
    /// `.LinkTitle`.
    pub link_title: String,
    /// `.Section`: the first path segment.
    pub section: String,
    /// `.Type`: front matter `type`, else the section, else `page`.
    pub r#type: String,
    /// Taxonomy and term pages: their taxonomy. Term pages also have their term.
    pub taxonomy: Option<TaxonomyIdx>,
    pub term: Option<TermIdx>,
    /// Standalone pages (404, sitemap, sitemap index, robots.txt): their only format.
    pub standalone: Option<FormatId>,
    /// The output formats, the primary first (none for pages without output).
    pub formats: Vec<FormatId>,
    /// Per format: the output file, link and resource directory, and the links (`None` when
    /// the page has no link: `build.render = never`, bundled pages).
    pub urls: Vec<PageUrl>,
    /// `.Parent` (`None` for the home page).
    pub parent: Option<PageId>,
    /// `.Ancestors`: the parent, its parent, … up to the home page.
    pub ancestors: Vec<PageId>,
    /// `.CurrentSection`: the page itself for branch pages.
    pub current_section: PageId,
    /// `.FirstSection`: the ancestor section at the root (the home page for root pages).
    pub first_section: PageId,
    /// `.Pages` and `.RegularPages` (default order; term pages list their members, standalone
    /// pages the site's), and `.Sections` (nodes only).
    pub pages: Vec<PageId>,
    pub regular_pages: Vec<PageId>,
    pub sections: Vec<PageId>,
    /// `.AllTranslations`: the page and its translations, in language order.
    pub translations: Vec<PageId>,
    /// `.GetTerms`: the page's terms per taxonomy (configuration order), in front matter order.
    pub terms: Vec<(TaxonomyIdx, TermIdx)>,
    /// `.Resources` before front matter `resources` metadata: bundle files (shared with the
    /// translations that have none of their own), then bundled pages.
    pub resources: Vec<ResourceId>,
}

/// A page's output in one format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageUrl {
    pub format: FormatId,
    pub paths: TargetPaths,
    /// The format's own `.RelPermalink`/`.Permalink` (`.OutputFormats.Get`); `None` when the
    /// page has no link.
    pub links: Option<Links>,
}

impl Page {
    /// A page of `kind` at `key`, before the structure is assembled.
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: PageId,
        lang: LangIdx,
        kind: PageKind,
        role: PageRole,
        key: ContentKey,
        source: Option<SourceFile>,
        path_info: PathInfo,
        meta: PageMeta,
    ) -> Self {
        Self {
            id,
            lang,
            kind,
            role,
            key,
            source,
            path_info,
            meta,
            title: String::new(),
            link_title: String::new(),
            section: String::new(),
            r#type: String::new(),
            taxonomy: None,
            term: None,
            standalone: None,
            formats: Vec::new(),
            urls: Vec::new(),
            parent: None,
            ancestors: Vec::new(),
            current_section: id,
            first_section: id,
            pages: Vec::new(),
            regular_pages: Vec::new(),
            sections: Vec::new(),
            translations: Vec::new(),
            terms: Vec::new(),
            resources: Vec::new(),
        }
    }

    /// The front matter params (after the cascade).
    #[must_use]
    pub fn params(&self) -> &Params {
        &self.meta.params
    }

    /// Go's `.Path` (`/posts/one`, `/` for the home page, `/_robots.txt`).
    #[must_use]
    pub fn path(&self) -> String {
        if self.standalone.is_some() {
            self.path_info.key.to_path()
        } else {
            self.key.to_path()
        }
    }

    /// `.Name`: the term as first written for term pages, else the title.
    #[must_use]
    pub fn name(&self) -> &str {
        if self.kind == PageKind::Term {
            &self.path_info.original.name
        } else {
            &self.title
        }
    }

    /// Whether the page is in its section's lists (`Local`) or in the site's (`Global`).
    #[must_use]
    pub fn listed(&self, scope: ListScope) -> bool {
        if self.standalone.is_some() || self.role != PageRole::Standalone {
            return false;
        }
        match self.meta.build.list {
            ListMode::Always => true,
            ListMode::Never => false,
            ListMode::Local => scope == ListScope::Local,
        }
    }

    /// Whether the page has a link of its own (`build.render` is not `never`).
    #[must_use]
    pub fn linked(&self) -> bool {
        self.meta.build.render != RenderMode::Never
    }

    /// Whether the page is written (`build.render = always`).
    #[must_use]
    pub fn rendered(&self) -> bool {
        self.meta.build.render == RenderMode::Always
    }

    /// The page's output in `format`.
    #[must_use]
    pub fn url(&self, format: FormatId) -> Option<&PageUrl> {
        self.urls.iter().find(|u| u.format == format)
    }

    /// `.RelPermalink`/`.Permalink` in the primary format.
    #[must_use]
    pub fn links(&self) -> Option<&Links> {
        self.urls.first().and_then(|u| u.links.as_ref())
    }

    /// Go's `Dir()` as a key: a bundle's (and a made page's) own key, a single file's
    /// directory.
    #[must_use]
    pub fn dir_key(&self) -> ContentKey {
        if self.path_info.kind.is_bundle() {
            self.key.clone()
        } else {
            self.key.parent().unwrap_or_default()
        }
    }
}

/// Where a list is shown: in a section (`.Pages`), or site-wide (`.Site.Pages`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListScope {
    Local,
    Global,
}

/// A file inside a bundle directory that is not a page of its own: bundle images and data, and
/// the content files of leaf bundles (then `page` is the bundled page).
#[derive(Clone, Debug)]
pub struct BundleResource {
    /// The resource's key: its path with extension (`blog/post/cover.jpg`).
    pub key: ContentKey,
    /// The language of the file (file name, else mount, else the default language; or, with
    /// `duplicateResourceFiles`, the language of the page it was copied for).
    pub lang: LangIdx,
    pub file: FileRef,
    pub info: PathInfo,
    pub page: Option<PageId>,
    /// A copy of another language's file for a page of this language
    /// (`duplicateResourceFiles`, multihost sites).
    pub copy_of: Option<ResourceId>,
    /// The page that owns the file: in the file's language, the page at the longest key above
    /// it in any language's tree. `None`: that language has no page there (the file is then
    /// neither published nor listed, as in Go).
    pub owner: Option<PageId>,
    /// `.Name` before front matter metadata: the path below the owner as written
    /// (`Sub/Photo.JPG`).
    pub name: String,
    /// The same, normalised (`sub/photo.jpg`).
    pub name_normalized: String,
    /// The owner's resource directory in its primary format.
    pub target_base: Option<ResourceBase>,
    /// Published with the owner (the owner is rendered and publishes its resources); else only
    /// when referenced.
    pub publish: bool,
    /// A resource a content adapter added (`file` is then the adapter): its content, name,
    /// title and params.
    pub adapter: Option<Arc<AddedResource>>,
}

impl BundleResource {
    /// A bundle file of `lang` before its owner is known.
    fn new(key: ContentKey, lang: LangIdx, file: FileRef, info: PathInfo) -> Self {
        Self {
            key,
            lang,
            file,
            info,
            page: None,
            copy_of: None,
            owner: None,
            name: String::new(),
            name_normalized: String::new(),
            target_base: None,
            publish: false,
            adapter: None,
        }
    }

    /// The file under `publishDir` (without a multihost language directory).
    #[must_use]
    pub fn target(&self) -> Option<OutputPath> {
        let base = self.target_base.as_ref()?;
        Some(OutputPath::new(&paths::join(&[
            "/",
            base.target.as_str(),
            &self.name,
        ])))
    }

    /// The link, relative to the site root and unescaped (`/posts/one/cover.jpg`).
    #[must_use]
    pub fn link(&self) -> Option<UrlPath> {
        let base = self.target_base.as_ref()?;
        Some(UrlPath::new(&paths::join(&[
            "/",
            base.link.as_str(),
            &self.name,
        ])))
    }
}

/// One language's part of the model.
#[derive(Clone, Debug)]
pub struct SiteModel {
    pub lang: LangIdx,
    /// The language's pages of their own (standalone pages included).
    pub tree: SiteTree,
    /// The language's bundle files by key (files of other languages are not in it).
    pub resources: BTreeMap<ContentKey, ResourceId>,
    /// The cascade handed down the tree.
    pub cascade: CascadeIndex,
    /// The home page.
    pub home: PageId,
    /// `.Site.Pages` and `.Site.RegularPages` (default order).
    pub pages: Vec<PageId>,
    pub regular_pages: Vec<PageId>,
    /// The regular pages listed locally (`build.list` `always` or `local`), default order:
    /// what `.RegularPagesRecursive` of the home page and of a section (filtered) read.
    pub regular_pages_local: Vec<PageId>,
    /// The configured taxonomies with their terms (configuration order).
    pub taxonomies: IdVec<TaxonomyIdx, Taxonomy>,
    /// `.Site.MainSections`: configured, else the root section with the most regular pages.
    pub main_sections: Vec<String>,
    /// `.Site.Lastmod`.
    pub last_mod: Option<Zoned>,
    /// The compiled `[permalinks]`.
    pub permalinks: PermalinkPatterns,
}

/// The site model: every page of every language in one arena.
#[derive(Clone, Debug)]
pub struct Model {
    pub config: Arc<Config>,
    pub pages: IdVec<PageId, Page>,
    pub sites: IdVec<LangIdx, SiteModel>,
    pub bundle_resources: IdVec<ResourceId, BundleResource>,
    /// `.Site.Data` (keys as written).
    pub data: Arc<Map>,
    /// Warnings and non-fatal errors of loading, in phase order.
    pub diagnostics: Vec<Diagnostic>,
    /// The lookup tables of page references.
    refs: refs::RefIndex,
}

impl Model {
    /// The page `id`.
    #[must_use]
    pub fn page(&self, id: PageId) -> &Page {
        &self.pages[id]
    }

    /// The index page of a bundled page's bundle: in the page's language, else in the first
    /// language (by weight) that has one.
    #[must_use]
    pub fn bundle_owner(&self, id: PageId) -> Option<PageId> {
        let p = &self.pages[id];
        let PageRole::Bundled { bundle } = &p.role else {
            return None;
        };
        self.sites[p.lang]
            .tree
            .get(bundle)
            .or_else(|| self.sites.iter().find_map(|s| s.tree.get(bundle)))
    }

    /// `.Name` of a page: a bundled page's is its normalised path in the bundle (`notes.md`).
    #[must_use]
    pub fn page_name(&self, id: PageId) -> &str {
        let p = &self.pages[id];
        if p.role != PageRole::Standalone
            && let Some(r) = self.sites[p.lang]
                .resources
                .get(&p.key)
                .map(|&r| &self.bundle_resources[r])
                .filter(|r| r.page == Some(id) && !r.name_normalized.is_empty())
        {
            return &r.name_normalized;
        }
        p.name()
    }

    /// Whether `ancestor` is a strict ancestor of `page` in the content tree (`.IsAncestor`,
    /// segment-wise).
    #[must_use]
    pub fn is_ancestor(&self, ancestor: PageId, page: PageId) -> bool {
        let (a, p) = (&self.pages[ancestor], &self.pages[page]);
        a.key != p.key && p.key.starts_with_segments(&a.key)
    }
}

/// Which unpublished content a build includes, and its "now".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadModelOptions {
    pub clock: Clock,
    pub content: ContentFilter,
}

impl LoadModelOptions {
    /// The configuration's `buildDrafts`/`buildFuture`/`buildExpired` at `clock`.
    #[must_use]
    pub fn from_config(cfg: &Config, clock: Clock) -> Self {
        Self {
            clock,
            content: cfg.content,
        }
    }
}
