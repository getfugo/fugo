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

/// Why the model could not be built.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error(transparent)]
    Vfs(#[from] VfsError),
    #[error("{path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    FrontMatter { path: PathBuf, message: String },
    #[error("{path}: {source}")]
    Page {
        path: PathBuf,
        #[source]
        source: PageError,
    },
    /// A Go-template content adapter (`_content.gotmpl`).
    #[error(
        "{0}: content adapters are Tera templates: port this Go template to \
         `_content.html` in the same directory (`add_page(page={{…}})`, \
         `add_resource(resource={{…}})`, `store_set`, `enable_all_languages()`; \
         https://github.com/getfugo/fugo/blob/main/docs/rust-port/template-api.md gives \
         the Go template functions with their Tera names)"
    )]
    GoContentAdapter(PathBuf),
    /// A page or resource a content adapter added that cannot be placed.
    #[error("{path}: {message}")]
    Adapter { path: PathBuf, message: String },
    #[error("{path}: no taxonomy is configured for {key:?}")]
    NoTaxonomy { path: PathBuf, key: String },
    #[error("site cascade: {0}")]
    SiteCascade(#[source] PageError),
    #[error("[permalinks] of language {lang}: {source}")]
    Permalinks {
        lang: String,
        #[source]
        source: PageError,
    },
    /// A page made by the build (a missing section, a term page) whose cascaded front matter
    /// is invalid, or a page whose URL cannot be made.
    #[error("page {path} ({lang}): {source}")]
    Node {
        path: String,
        lang: String,
        #[source]
        source: PageError,
    },
    #[error(transparent)]
    Data(#[from] DataError),
}

/// A page the build filter removed: node dates still count its dates (Go aggregates dates
/// before it removes drafts, future and expired content).
#[derive(Clone, Debug)]
pub(crate) struct Removed {
    pub lang: LangIdx,
    pub key: ContentKey,
    pub kind: PageKind,
    pub dates: Dates,
}

/// The content and data of a project, read and decoded (phase A4), before the model is
/// assembled; content adapters are listed, not run.
#[derive(Clone, Debug)]
pub struct Captured {
    capture: capture::Capture,
    data: Data,
}

impl Captured {
    /// The content adapters (`_content.html`), by directory, then language.
    #[must_use]
    pub fn adapters(&self) -> &[ContentAdapter] {
        &self.capture.adapters
    }
}

/// What content adapters added: their pages and page resources, in the order they were
/// added.
#[derive(Clone, Debug, Default)]
pub struct Added {
    pub pages: Vec<AddedPage>,
    pub resources: Vec<AddedResource>,
}

/// A page an adapter added with `add_page`.
#[derive(Clone, Debug)]
pub struct AddedPage {
    /// The adapter (an index into [`Captured::adapters`]).
    pub adapter: usize,
    /// The language the adapter ran for.
    pub lang: LangIdx,
    pub page: Arc<AdapterPage>,
}

/// A page resource an adapter added with `add_resource` (Go's `ResourceConfig`).
#[derive(Clone, Debug)]
pub struct AddedResource {
    /// The adapter (an index into [`Captured::adapters`]).
    pub adapter: usize,
    /// The language the adapter ran for.
    pub lang: LangIdx,
    /// The resource's path below the content root, normalised, without a leading slash
    /// (`news/p1/cover.jpg`): it belongs to the page at the longest key above it.
    pub path: String,
    /// `name` (`None`: the path below the page).
    pub name: Option<String>,
    /// `title` (`None`: the name).
    pub title: Option<String>,
    pub params: Params,
    pub content: AddedContent,
}

/// The content of a resource an adapter added.
#[derive(Clone, Debug)]
pub enum AddedContent {
    /// A string `content.value`, of `content.mediaType` (`None`: the type of the path's
    /// extension); published below its page like a bundle file.
    Text {
        text: Arc<str>,
        media_type: Option<String>,
    },
    /// A resource the adapter got (`get_asset`, `get_remote`, …): Go uses the resource itself,
    /// so it keeps its own file and link (relative to the site root, not to the page).
    Resource {
        body: AddedBody,
        media_type: String,
        target: OutputPath,
        link: UrlPath,
    },
}

/// Where the bytes of an added resource are.
#[derive(Clone, Debug)]
pub enum AddedBody {
    File(PathBuf),
    Bytes(Arc<[u8]>),
}

/// Reads the content and data of `cfg`'s project (phase A4, see the crate docs).
///
/// # Errors
/// A content or data file that cannot be read or decoded, front matter capture overrides of
/// the wrong shape, or a Go-template content adapter (`_content.gotmpl`).
pub fn capture_content(cfg: &Config, vfs: &Vfs) -> Result<Captured, ModelError> {
    let (captured, data) = rayon::join(|| capture::capture(cfg, vfs), || data::load(vfs));
    Ok(Captured {
        capture: captured?,
        data: data?,
    })
}

/// Builds the model of `cfg`'s project from content without running its content adapters
/// ([`capture_content`], then [`assemble`] with nothing [`Added`]).
///
/// # Errors
/// See [`capture_content`] and [`assemble`].
pub fn load_model(cfg: Arc<Config>, vfs: &Vfs, o: &LoadModelOptions) -> Result<Model, ModelError> {
    let captured = capture_content(&cfg, vfs)?;
    assemble(cfg, captured, Added::default(), o)
}

/// Builds the model from captured content and the pages content adapters added: the added
/// pages go after the content files (a file wins a key both claim, with a warning).
///
/// A key (or resource path) both a content file and an adapter claim is the file's; one two
/// adapters claim is the later adapter's, in the place of the earlier one's (both with a
/// warning). This is Go's order with one collector worker (`pages_capture.go` `collectDirDir`
/// queues a directory's adapters before its files and its subdirectories, and `content_map.go`
/// `insertPageWithLock`/`insertResourceWithLock` keep the last insert); with several workers
/// Go's result depends on scheduling.
///
/// # Errors
/// Invalid front matter (reserved keys of the wrong shape, a bad cascade or date
/// configuration), a page an adapter added that cannot be placed, a taxonomy kind without a
/// taxonomy, a `[permalinks]` pattern that does not parse, or a URL that cannot be made.
pub fn assemble(
    cfg: Arc<Config>,
    captured: Captured,
    added: Added,
    o: &LoadModelOptions,
) -> Result<Model, ModelError> {
    let Captured {
        capture: mut captured,
        data,
    } = captured;
    if !added.pages.is_empty() || !added.resources.is_empty() {
        let parser = PathParser::from_config(&cfg);
        // (language, key) → place in `captured.pages` of the adapter pages added so far.
        let mut page_at: HashMap<(LangIdx, ContentKey), usize> = HashMap::new();
        for a in added.pages {
            let adapter = &captured.adapters[a.adapter];
            let page = capture::adapter_page(adapter, a.lang, a.page, &parser)?;
            // A file claiming the key wins in `tree::place`; an earlier adapter's page is
            // replaced (one adapter's repeats are already its last `add_page`).
            match page_at.entry((page.lang, page.source.info.key.clone())) {
                Entry::Occupied(at) => {
                    let earlier = &mut captured.pages[*at.get()];
                    captured.diagnostics.push(later_adapter_wins(
                        "content",
                        &page.source.info.key,
                        &page.source.file.abs,
                        &earlier.source.file.abs,
                    ));
                    *earlier = page;
                }
                Entry::Vacant(at) => {
                    at.insert(captured.pages.len());
                    captured.pages.push(page);
                }
            }
        }
        // (language, key) → place in `captured.resources`: the first file of a key, then the
        // adapter resources.
        let mut resource_at: HashMap<(LangIdx, ContentKey), usize> = HashMap::new();
        if !added.resources.is_empty() {
            for (i, f) in captured.resources.iter().enumerate() {
                resource_at.entry((f.lang, f.info.key.clone())).or_insert(i);
            }
        }
        for a in added.resources {
            let adapter = &captured.adapters[a.adapter];
            let r = capture::adapter_resource(adapter, a, &parser)?;
            let taken = match resource_at.entry((r.lang, r.info.key.clone())) {
                Entry::Occupied(at) => Some(&mut captured.resources[*at.get()]),
                Entry::Vacant(at) => {
                    at.insert(captured.resources.len());
                    None
                }
            };
            match taken {
                Some(f) if f.adapter.is_none() => {
                    captured.diagnostics.push(
                        Diagnostic::warning(format!(
                            "duplicate resource path {:?}: {} is used, the resource {} adds \
                             is ignored",
                            r.info.key.to_path(),
                            f.file.abs.display(),
                            r.file.abs.display()
                        ))
                        .with_id("duplicate-resource-path"),
                    );
                }
                Some(f) => {
                    captured.diagnostics.push(later_adapter_wins(
                        "resource",
                        &r.info.key,
                        &r.file.abs,
                        &f.file.abs,
                    ));
                    *f = r;
                }
                None => captured.resources.push(r),
            }
        }
    }
    let assembly = tree::place(&cfg, captured)?;
    let cascades = meta::cascade_indexes(&cfg, &assembly)?;
    let (metas, meta_diags) = meta::metas(&cfg, &assembly, &cascades)?;

    let mut diagnostics = assembly.diagnostics;
    diagnostics.extend(meta_diags);

    // Filter: removed pages take the bundle files below them (same language) with them.
    let mut removed: Vec<Removed> = Vec::new();
    let mut keep = vec![true; assembly.pages.len()];
    let mut metas: Vec<Option<PageMeta>> = metas.into_iter().map(Some).collect();
    for (i, p) in assembly.pages.iter().enumerate() {
        if p.role != PageRole::Standalone {
            continue;
        }
        let site = &cfg.sites[p.page.lang];
        let Some(meta) = metas[i].as_mut() else {
            continue;
        };
        match o.verdict(p.kind, !site.disable_kinds.contains(p.kind), meta) {
            Verdict::Build => {}
            Verdict::Disable => meta.build = filter::DISABLED,
            Verdict::Remove => {
                keep[i] = false;
                removed.push(Removed {
                    lang: p.page.lang,
                    key: p.key.clone(),
                    kind: p.kind,
                    dates: meta.dates.clone(),
                });
            }
        }
    }
    let below_removed = |lang: LangIdx, key: &ContentKey| {
        removed
            .iter()
            .any(|r| r.lang == lang && r.key != *key && key.starts_with_segments(&r.key))
    };

    let mut pages: IdVec<PageId, Page> = IdVec::new();
    let mut bundle_resources: IdVec<ResourceId, BundleResource> = IdVec::new();
    let mut sites: IdVec<LangIdx, SiteModel> = cascades
        .into_iter()
        .enumerate()
        .map(|(i, cascade)| SiteModel {
            lang: <LangIdx as ssg_base::Idx>::from_index(i),
            tree: SiteTree::default(),
            resources: BTreeMap::new(),
            cascade,
            home: PageId::from_raw(0),
            pages: Vec::new(),
            regular_pages: Vec::new(),
            regular_pages_local: Vec::new(),
            taxonomies: IdVec::new(),
            main_sections: Vec::new(),
            last_mod: None,
            permalinks: PermalinkPatterns::default(),
        })
        .collect();
    for (i, p) in assembly.pages.into_iter().enumerate() {
        let lang = p.page.lang;
        let bundled = p.role != PageRole::Standalone;
        if !keep[i] || (bundled && below_removed(lang, &p.key)) {
            continue;
        }
        let Some(meta) = metas[i].take() else {
            continue;
        };
        let id = pages.next_id();
        let source = p.page.source;
        if bundled {
            let mut r = BundleResource::new(
                p.key.clone(),
                lang,
                source.file.clone(),
                source.file_info.clone(),
            );
            r.page = Some(id);
            let rid = bundle_resources.push(r);
            sites[lang].resources.insert(p.key.clone(), rid);
        } else {
            sites[lang].tree.insert(p.key.clone(), id);
        }
        let path_info = source.info.clone();
        pages.push(Page::new(
            id,
            lang,
            p.kind,
            p.role,
            p.key,
            Some(source),
            path_info,
            meta,
        ));
    }
    for r in assembly.resources {
        if below_removed(r.lang, &r.info.key) {
            continue;
        }
        let key = r.info.key.clone();
        let mut br = BundleResource::new(key.clone(), r.lang, r.file, r.info);
        br.adapter = r.adapter;
        let rid = bundle_resources.push(br);
        sites[r.lang].resources.insert(key, rid);
    }

    diagnostics.extend(data.diagnostics);
    let parser = Arc::new(PathParser::from_config(&cfg));
    let mut model = Model {
        config: cfg,
        pages,
        sites,
        bundle_resources,
        data: Arc::new(data.map),
        diagnostics,
        refs: refs::RefIndex::new(parser),
    };
    nodes::assemble(&mut model, o, &removed)?;
    Ok(model)
}

/// The warning for a page or resource path (`what`: `content`, `resource`) two content
/// adapters add: the one that runs `later` replaces the `earlier` one's.
fn later_adapter_wins(
    what: &str,
    key: &ContentKey,
    later: &std::path::Path,
    earlier: &std::path::Path,
) -> Diagnostic {
    Diagnostic::warning(format!(
        "duplicate {what} path {:?}: {} is used, {} is ignored (of two content adapters, the \
         one that runs later wins)",
        key.to_path(),
        later.display(),
        earlier.display()
    ))
    .with_id(format!("duplicate-{what}-path"))
}
