//! The render [`Session`] (REWRITE_PLAN.md §2.6): templates, views and the named mutable build
//! state of phases C–E. `Session::{new, render_content, freeze_views, render_job}` are frozen by
//! T38; their bodies are the skeleton's.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock, Weak};

use rayon::prelude::*;
use ssg_base::diag::{Diagnostic, Diagnostics};
use ssg_base::paths::{ContentKey, OutputPath};
use ssg_base::{Clock, FormatId, IdVec, Idx, LangIdx, PageId, PageKind};
use ssg_funcs::{EnvAllowlist, Locales, PureEnv, register_placeholders, register_pure};
use ssg_highlight::Highlight;
use ssg_images::{ImageCache, ImageQueue, Imaging};
use ssg_layouts::{
    EmbeddedHooks, LayoutQuery, LayoutStore, Origin, Selection, Selections, StandaloneKind,
    TemplateName, TemplateRole, Templates,
};
use ssg_markup::{Fragments, MarkdownOptions};
use ssg_resources::{ResourceStore, StoreConfig};
use ssg_site::{Model, Page, PageUrl};
use ssg_sitefuncs::Handles;
use ssg_vfs::Vfs;
use ssg_view::views::BuildView;
use ssg_view::{
    ContentError, ContentRenderer, Contents, Deferred, DeferredRegistry, ExpandedSource,
    HookVariant, NavSite, PageStores, PaginationRecorder, Phase, RenderScope, RenderStringOptions,
    RenderedContent, SCOPE_KEY, ViewCache, ViewInputs, page_target,
};

use crate::job::{AliasPlan, Job, JobOrder, Output};
use crate::memo::ContentStore;
use crate::tokens::Inclusions;
use crate::{RenderError, content, i18n};

mod functions;
mod jobs;
mod waves;

/// The project inputs a session renders besides the model: the file system and the scanned
/// layouts.
#[derive(Clone, Debug)]
pub struct Project {
    pub vfs: Arc<Vfs>,
    pub layouts: Arc<LayoutStore>,
}

/// Build-wide render settings.
#[derive(Clone, Copy, Debug)]
pub struct RenderOptions {
    /// `now()` and the build's "now" (`--clock`).
    pub clock: Clock,
    /// The build runs in the `server` command (`build.is_server`).
    pub server: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            clock: Clock::system(),
            server: false,
        }
    }
}

/// The render state of one build. Created once; phases call it in order:
/// [`render_content`](Self::render_content) (C1), [`freeze_views`](Self::freeze_views) (D),
/// then [`render_job`](Self::render_job) for every job of waves 1 and 2 (E2, E3).
pub struct Session {
    model: Arc<Model>,
    project: Project,
    templates: Arc<Templates>,
    views: Arc<ViewCache>,
    pagination: Arc<PaginationRecorder>,
    /// What the site functions hold (their `Arc`s are the session's named mutable state).
    handles: Handles,
    selections: BTreeMap<(PageId, FormatId), Selection>,
    /// Front matter alias files per language (`ssg_nav::page_aliases`).
    aliases: IdVec<LangIdx, Vec<AliasPlan>>,
    /// The hook variants content is rendered in: `Html`, then `Format(F)` for every format F
    /// with a `_markup/*.<F>.*` hook.
    variants: Vec<HookVariant>,
    html_format: FormatId,
    /// The memo cells of the content phase.
    cells: ContentStore,
    /// Per language: Markdown options, `useEmbedded`, the highlighter (built at the first
    /// fence no hook handles).
    markdown: IdVec<LangIdx, MarkdownOptions>,
    embedded_hooks: IdVec<LangIdx, EmbeddedHooks>,
    highlighters: IdVec<LangIdx, OnceLock<Arc<Highlight>>>,
    /// The Tera names of embedded templates (the embedded table hook is written natively).
    embedded: BTreeSet<TemplateName>,
    inclusions: Inclusions,
    /// Phase C1's result, frozen into the views in phase D.
    contents: OnceLock<BTreeMap<HookVariant, Contents>>,
    build_info: tera::Value,
    diagnostics: Arc<Diagnostics>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("pages", &self.model.pages.len())
            .field("templates", &self.templates)
            .finish_non_exhaustive()
    }
}

/// The keys of `site` a content adapter does not see: the page lists, which Go's adapters
/// cannot read either ("cannot be called before the site is fully initialized").
const ADAPTER_HIDDEN_SITE_KEYS: [&str; 8] = [
    "home",
    "pages",
    "regular_pages",
    "all_pages",
    "sections",
    "main_sections",
    "taxonomies",
    "menus",
];

/// The pure-function environment of a model.
fn pure_env(model: &Model, o: &RenderOptions, diagnostics: &Arc<Diagnostics>) -> PureEnv {
    let cfg = &model.config;
    let site = cfg.default_site();
    let mut env = PureEnv::new(&site.language.key);
    env.clock = o.clock;
    env.time_zone = site.language.time_zone.clone();
    env.locales = Locales::new(
        &site.language.key,
        cfg.sites.iter().map(|s| s.language.key.as_str()),
    );
    env.title_style = site.titles.case_style;
    env.path_case = site.urls.path_case;
    env.accents = site.urls.accents;
    env.diagnostics = Arc::clone(diagnostics);
    env.project_dir = Some(cfg.project_dir.clone());
    // The config compiled these patterns already; `none` and empty entries allow nothing.
    env.getenv = EnvAllowlist::new(
        cfg.security
            .getenv
            .patterns()
            .iter()
            .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case("none")),
    )
    .unwrap_or_default();
    env.env_file = cfg.env_file.vars();
    env
}

/// The outputs a page is rendered in (none unless it is written), the primary first.
fn outputs(p: &Page) -> impl Iterator<Item = &PageUrl> {
    p.urls
        .iter()
        .filter(move |u| p.rendered() && u.links.is_some())
}

/// The output formats page `p` is rendered in (none unless it is written), the primary first:
/// the (page, format) pairs that get a layout job (`templates check` lists their
/// lookups).
pub fn rendered_formats(p: &Page) -> impl Iterator<Item = FormatId> + '_ {
    outputs(p).map(|u| u.format)
}

/// The layout query of page `p` in `format` (`path` is [`lookup_path`]`(p)`).
#[must_use]
pub fn layout_query<'a>(p: &'a Page, path: &'a ContentKey, format: FormatId) -> LayoutQuery<'a> {
    let standalone = matches!(
        p.kind,
        PageKind::NotFound | PageKind::Sitemap | PageKind::SitemapIndex | PageKind::RobotsTxt
    );
    LayoutQuery {
        path,
        kind: Some(p.kind),
        layout: p.meta.layout.as_deref(),
        exact_layout: false,
        lang: (!standalone).then_some(p.lang),
        format,
    }
}

/// The lookup path of a page: its key with the first segment replaced by its type when that
/// differs from the section. Layouts, shortcodes and render hooks are looked up from it.
#[must_use]
pub fn lookup_path(p: &Page) -> ContentKey {
    if p.section.is_empty() || p.r#type == p.section || p.r#type == "page" {
        return p.key.clone();
    }
    let rest: Vec<&str> = p.key.segments().skip(1).collect();
    let mut path = p.r#type.clone();
    for s in rest {
        path.push('/');
        path.push_str(s);
    }
    ContentKey::from_source(&path)
}

fn is_standalone(kind: PageKind) -> bool {
    matches!(
        kind,
        PageKind::NotFound | PageKind::Sitemap | PageKind::SitemapIndex | PageKind::RobotsTxt
    )
}

impl Session {
    /// Loads the templates and prepares the views of `model`, in the plan's order: the renderer
    /// slot is created empty → the site functions are registered (`ssg_sitefuncs::register`
    /// after the pure functions) → the layouts are loaded (validating every template) →
    /// `Arc::new(Session)` → the slot is set to the session.
    ///
    /// `Handles` gets one [`ImageQueue`] (with the `[caches.images]` file cache) shared with the
    /// resource store, the translations of the i18n files, and a highlighter shared with the
    /// Markdown of every language with the default `[markup.highlight]`.
    ///
    /// # Errors
    /// [`RenderError::View`] (invalid `resources` front matter), [`RenderError::Nav`] (an
    /// alias that cannot be written), [`RenderError::Template`] (Tera load errors),
    /// [`RenderError::I18n`] (an i18n file that does not load).
    pub fn new(
        project: Project,
        model: Arc<Model>,
        o: &RenderOptions,
    ) -> Result<Arc<Self>, RenderError> {
        Self::with_functions(project, model, o, &|_, _| {})
    }

    /// The scope a phase C1 computation of page `p` in variant `v` starts from.
    fn root_scope(&self, p: &Page, v: HookVariant) -> RenderScope {
        RenderScope {
            phase: Phase::Content,
            variant: v,
            ..RenderScope::layout(p.id, p.lang, self.variant_format(v), None)
        }
    }

    /// Phase D: freezes the Full view generations (after [`render_content`](Self::render_content)).
    ///
    /// # Errors
    /// [`RenderError::Phase`] when the content was not rendered.
    pub fn freeze_views(&self) -> Result<(), RenderError> {
        let contents = self
            .contents
            .get()
            .ok_or(RenderError::Phase("freeze_views before render_content"))?;
        self.views.freeze(contents);
        Ok(())
    }

    /// The model the session renders.
    #[must_use]
    pub fn model(&self) -> &Arc<Model> {
        &self.model
    }

    /// The frozen views.
    #[must_use]
    pub fn views(&self) -> &Arc<ViewCache> {
        &self.views
    }

    /// Warnings and errors of the render.
    #[must_use]
    pub fn diagnostics(&self) -> &Arc<Diagnostics> {
        &self.diagnostics
    }

    /// The page stores (`.Store`).
    #[must_use]
    pub fn page_stores(&self) -> &Arc<PageStores> {
        &self.handles.stores
    }

    /// The named mutable build state the site functions share (resource store, image queue,
    /// pagination recorder, deferred registry, …); phases E4–E6 read it.
    #[must_use]
    pub fn handles(&self) -> &Handles {
        &self.handles
    }

    /// The hook variants content is rendered in (`Html` first).
    #[must_use]
    pub fn variants(&self) -> &[HookVariant] {
        &self.variants
    }

    /// The loaded templates.
    #[must_use]
    pub fn templates(&self) -> &Templates {
        &self.templates
    }

    pub(crate) fn build_info(&self) -> &tera::Value {
        &self.build_info
    }

    pub(crate) fn stores(&self) -> &PageStores {
        &self.handles.stores
    }

    pub(crate) fn cells(&self) -> &ContentStore {
        &self.cells
    }

    pub(crate) fn inclusions(&self) -> &Inclusions {
        &self.inclusions
    }

    pub(crate) fn markdown(&self, lang: LangIdx) -> &MarkdownOptions {
        &self.markdown[lang]
    }

    pub(crate) fn embedded_hooks(&self, lang: LangIdx) -> EmbeddedHooks {
        self.embedded_hooks[lang]
    }

    pub(crate) fn highlighter(&self, lang: LangIdx) -> &OnceLock<Arc<Highlight>> {
        &self.highlighters[lang]
    }

    pub(crate) fn html_format(&self) -> FormatId {
        self.html_format
    }

    /// Whether `t` is an embedded template.
    pub(crate) fn is_embedded(&self, t: &TemplateName) -> bool {
        self.embedded.contains(t)
    }

    /// `v` if content is rendered in it, else `Html`.
    pub(crate) fn known_variant(&self, v: HookVariant) -> HookVariant {
        if self.variants.contains(&v) {
            v
        } else {
            HookVariant::Html
        }
    }

    /// The output format shortcodes and hooks are looked up with in variant `v`.
    pub(crate) fn variant_format(&self, v: HookVariant) -> FormatId {
        match v {
            HookVariant::Html => self.html_format,
            HookVariant::Format(f) => f,
        }
    }

    pub(crate) fn variant_name(&self, v: HookVariant) -> String {
        self.model
            .config
            .output_formats
            .get(self.variant_format(v))
            .name
            .clone()
    }
}

/// `Html`, then `Format(F)` for every output format F other than HTML that has a render hook.
fn hook_variants(layouts: &LayoutStore, html: FormatId) -> Vec<HookVariant> {
    let formats: BTreeSet<FormatId> = layouts
        .templates()
        .filter(|t| matches!(t.role, TemplateRole::Hook { .. }))
        .filter_map(|t| t.format)
        .filter(|&f| f != html)
        .collect();
    std::iter::once(HookVariant::Html)
        .chain(formats.into_iter().map(HookVariant::Format))
        .collect()
}

impl ContentRenderer for Session {
    fn content(
        &self,
        p: PageId,
        v: HookVariant,
        s: &RenderScope,
    ) -> Result<Arc<RenderedContent>, ContentError> {
        self.content_of(p, v, s)
    }

    fn fragments(&self, p: PageId, s: &RenderScope) -> Result<Arc<Fragments>, ContentError> {
        self.fragments_of(p, s)
    }

    /// In the content phase, the returned `markdown` is an inclusion token that the expanding
    /// page replaces by `p`'s expanded source (a site function prints `markdown` as it is);
    /// elsewhere it is `p`'s source with the shortcode outputs in place.
    fn render_shortcodes(
        &self,
        p: PageId,
        s: &RenderScope,
    ) -> Result<Arc<ExpandedSource>, ContentError> {
        self.shortcodes_of(p, s)
    }

    fn render_markdown(
        &self,
        md: &str,
        o: RenderStringOptions,
        s: &RenderScope,
    ) -> Result<String, ContentError> {
        self.markdown_in_scope(md, o, s)
    }

    fn render_template(
        &self,
        t: &TemplateName,
        mut ctx: tera::Context,
        s: &RenderScope,
    ) -> Result<String, ContentError> {
        let mut child = s.child();
        // A `partial()` opened its `return_value` frame on `s`; the template it renders owns it.
        child.frame = s.frame;
        if child.too_deep() {
            return Err(ContentError::TooDeep {
                limit: ssg_view::MAX_DEPTH,
            });
        }
        ctx.insert_value(SCOPE_KEY, child.to_value());
        self.templates
            .tera()
            .render(t.as_str(), &ctx)
            .map_err(|e| ContentError::Render(crate::shortcode::error_chain(&e)))
    }
}
