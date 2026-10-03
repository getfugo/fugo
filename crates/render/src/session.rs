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

    /// [`new`](Self::new), with `extra` registering more template functions after the site
    /// functions (tests; a function registered here replaces one of the same name).
    ///
    /// # Errors
    /// As [`new`](Self::new).
    pub fn with_functions(
        project: Project,
        model: Arc<Model>,
        o: &RenderOptions,
        extra: &dyn Fn(&mut tera::Tera, &Handles),
    ) -> Result<Arc<Self>, RenderError> {
        let diagnostics = Arc::new(Diagnostics::new(model.config.ignore_logs.iter()));
        let cfg = &model.config;
        let imaging = Imaging::from_config(&cfg.imaging).unwrap_or_else(|e| {
            diagnostics.push(Diagnostic::error(format!("[imaging]: {e}")));
            Imaging::default()
        });
        // One image queue: the store registers processed images in it, `resize` & co. enqueue
        // into it, and phase E6 processes it into `[caches.images]`.
        let images = Arc::new(ImageQueue::new(
            imaging,
            cfg.caches.get("images").map(ImageCache::from_config),
        ));
        let store = Arc::new(ResourceStore::new(StoreConfig::from_config(
            cfg,
            Some(Arc::clone(&project.vfs)),
            Some(Arc::clone(&images)),
        )));
        let translations = Arc::new(i18n::load(&project.vfs, cfg)?);
        let (menus, menu_diagnostics) =
            ssg_nav::build_menus(&NavSite::new(Arc::clone(&model)), &model.config);
        for d in menu_diagnostics {
            diagnostics.push(d);
        }
        let views = Arc::new(ViewCache::new(ViewInputs {
            model: Arc::clone(&model),
            store: Arc::clone(&store),
            menus: Arc::new(menus),
        })?);
        let pagination = Arc::new(PaginationRecorder::default());
        let aliases = model
            .config
            .sites
            .ids()
            .map(|l| ssg_nav::page_aliases(views.nav(), &model.config, l))
            .collect::<Result<_, _>>()?;

        let mut selections = BTreeMap::new();
        for p in &model.pages {
            let path = lookup_path(p);
            for out in outputs(p) {
                match project.layouts.select(&layout_query(p, &path, out.format)) {
                    Some(s) => {
                        selections.insert((p.id, out.format), s);
                    }
                    None if is_standalone(p.kind) => {}
                    None => diagnostics.push(Diagnostic::warning(format!(
                        "no layout for {} page {} in format {}",
                        p.kind.as_str(),
                        p.key.to_path(),
                        model.config.output_formats.get(out.format).name
                    ))),
                }
            }
        }
        let sel: Selections = selections.values().cloned().collect();

        let html_format = cfg
            .output_formats
            .by_name("html")
            .unwrap_or(FormatId::from_raw(0));
        let variants = hook_variants(&project.layouts, html_format);
        let embedded = project
            .layouts
            .templates()
            .filter(|t| t.origin == Origin::Embedded)
            .map(|t| t.render_name().clone())
            .collect();
        let highlight = Arc::new(Highlight::new(&cfg.default_site().markup.highlight));
        let renderer: Arc<OnceLock<Weak<dyn ContentRenderer>>> = Arc::new(OnceLock::new());
        let templates_slot: Arc<OnceLock<Weak<Templates>>> = Arc::new(OnceLock::new());
        let handles = Handles {
            model: Arc::clone(&model),
            views: Arc::clone(&views),
            store,
            images,
            stores: Arc::new(PageStores::new(model.pages.len())),
            pagination: Arc::clone(&pagination),
            deferred: Arc::new(DeferredRegistry::default()),
            css_purges: Arc::default(),
            menus: Arc::clone(views.menus()),
            related: Arc::new(ssg_sitefuncs::RelatedCache::default()),
            i18n: translations,
            diagnostics: Arc::clone(&diagnostics),
            highlight: Arc::clone(&highlight),
            renderer: Arc::clone(&renderer),
            templates: Arc::clone(&templates_slot),
            frames: Arc::default(),
            partial_cache: Arc::default(),
            adapters: Arc::default(),
        };

        let pure = Arc::new(pure_env(&model, o, &diagnostics));
        let templates = ssg_layouts::load(Arc::clone(&project.layouts), &sel, &|t| {
            register_placeholders(t);
            register_pure(t, &pure);
            ssg_sitefuncs::register(t, &handles);
            extra(t, &handles);
        })?;
        let session = Arc::new(Self {
            build_info: tera::Value::from_serializable(&BuildView::new(&model.config, o.server)),
            cells: ContentStore::new(model.pages.len(), &variants),
            markdown: cfg.sites.iter().map(content::markdown_options).collect(),
            embedded_hooks: cfg
                .sites
                .iter()
                .map(|s| EmbeddedHooks::of(s, cfg))
                .collect(),
            // A language whose `[markup.highlight]` is the default site's shares the `highlight`
            // filter's highlighter (the syntax sets are loaded once).
            highlighters: cfg
                .sites
                .iter()
                .map(|s| {
                    let cell = OnceLock::new();
                    if s.markup.highlight == cfg.default_site().markup.highlight {
                        let _ = cell.set(Arc::clone(&highlight));
                    }
                    cell
                })
                .collect(),
            model,
            project,
            templates: {
                let templates = Arc::new(templates);
                let _ = templates_slot.set(Arc::downgrade(&templates));
                templates
            },
            views,
            pagination,
            handles,
            selections,
            aliases,
            variants,
            html_format,
            embedded,
            inclusions: Inclusions::default(),
            contents: OnceLock::new(),
            diagnostics,
        });
        let weak: Weak<Self> = Arc::downgrade(&session);
        let weak: Weak<dyn ContentRenderer> = weak;
        let _ = renderer.set(weak);
        Ok(session)
    }

    /// Phase C1: renders the content of every page with a content file (bundled content pages
    /// included) in every hook variant. A second call does nothing.
    ///
    /// # Errors
    /// [`RenderError::Content`] of the first page (in page order) whose content fails.
    pub fn render_content(&self) -> Result<(), RenderError> {
        if self.contents.get().is_some() {
            return Ok(());
        }
        let model = &self.model;
        let per_page: Vec<Vec<Option<Arc<RenderedContent>>>> = model
            .pages
            .as_slice()
            .par_iter()
            .map(|p| {
                self.variants
                    .iter()
                    .map(|&v| {
                        if p.source.is_none() {
                            return Ok(None);
                        }
                        let scope = self.root_scope(p, v);
                        self.content_of(p.id, v, &scope)
                            .map(Some)
                            .map_err(|source| RenderError::Content {
                                page: p.source.as_ref().map_or_else(
                                    || p.key.to_path(),
                                    |s| s.file.abs.display().to_string(),
                                ),
                                source: Box::new(source),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<_, _>>()?;
        let mut all = BTreeMap::new();
        for (i, &v) in self.variants.iter().enumerate() {
            let c: Contents = per_page.iter().map(|row| row[i].clone()).collect();
            all.insert(v, c);
        }
        let _ = self.contents.set(all);
        Ok(())
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

    /// Phase E: renders one job. Pure: no I/O; the outputs go to the publisher.
    ///
    /// # Errors
    /// A template error ([`RenderError::Render`]) or an unknown page or format.
    pub fn render_job(&self, job: &Job) -> Result<Vec<Output>, RenderError> {
        if !self.views.is_frozen() {
            return Err(RenderError::Phase("render_job before freeze_views"));
        }
        let model = &self.model;
        match *job {
            Job::Page { page, format } | Job::Standalone { page, format } => {
                let p = &model.pages[page];
                let Some(out) = outputs(p).find(|o| o.format == format) else {
                    return Err(RenderError::NoOutput { page, format });
                };
                self.render_layout(job, page, format, None, out.paths.target.clone())
            }
            Job::Pager {
                page,
                format,
                number,
            } => {
                let (target, _) = page_target(model, page, format, Some(number))?;
                self.render_layout(job, page, format, Some(number), target.target)
            }
            Job::Alias(ref a) => {
                let to = self.permalink(a.to, a.format)?;
                self.render_alias(job, &a.from, &to, Some(a.to), a.format)
            }
            Job::PagerAlias { page, format } => {
                let (target, _) = page_target(model, page, format, Some(1))?;
                let to = self.permalink(page, format)?;
                self.render_alias(job, &target.target, &to, Some(page), format)
            }
            Job::LanguageRedirect => {
                let Some(a) = self.language_redirect() else {
                    return Ok(Vec::new());
                };
                let to = self.permalink(a.to, a.format)?;
                self.render_alias(job, &a.from, &to, None, a.format)
            }
        }
    }

    /// The file `job` writes, without rendering it: `None` when it writes nothing (a page
    /// without a layout, an alias without an alias template, a language redirect without a
    /// home output). Phase E resolves target collisions with it before rendering.
    ///
    /// # Errors
    /// An unknown page or format, or a pager whose file cannot be made.
    pub fn target(&self, job: &Job) -> Result<Option<OutputPath>, RenderError> {
        let model = &self.model;
        let has_layout =
            |page: PageId, format: FormatId| self.selections.contains_key(&(page, format));
        Ok(match *job {
            Job::Page { page, format } | Job::Standalone { page, format } => {
                if !has_layout(page, format) {
                    return Ok(None);
                }
                let out = outputs(&model.pages[page])
                    .find(|o| o.format == format)
                    .ok_or(RenderError::NoOutput { page, format })?;
                Some(out.paths.target.clone())
            }
            Job::Pager {
                page,
                format,
                number,
            } => {
                if !has_layout(page, format) {
                    return Ok(None);
                }
                Some(page_target(model, page, format, Some(number))?.0.target)
            }
            Job::Alias(ref a) => self.alias_template().map(|_| a.from.clone()),
            Job::PagerAlias { page, format } => match self.alias_template() {
                Some(_) => Some(page_target(model, page, format, Some(1))?.0.target),
                None => None,
            },
            Job::LanguageRedirect => match (self.alias_template(), self.language_redirect()) {
                (Some(_), Some(a)) => Some(a.from),
                _ => None,
            },
        })
    }

    /// Phase E5: renders the template of a `defer(...)` call registered under `key`, with
    /// `data`, `site` (the default language's), `build` and a scope in phase `Deferred`.
    ///
    /// # Errors
    /// [`RenderError::Phase`] before [`freeze_views`](Self::freeze_views), or
    /// [`RenderError::Deferred`] when the template fails.
    pub fn render_deferred(&self, key: &str, d: &Deferred) -> Result<String, RenderError> {
        if !self.views.is_frozen() {
            return Err(RenderError::Phase("render_deferred before freeze_views"));
        }
        let lang = LangIdx::from_index(0);
        let home = self.model.sites[lang].home;
        let scope = RenderScope {
            phase: Phase::Deferred,
            ..RenderScope::layout(home, lang, self.html_format, None)
        };
        let generation = self.views.generation(Phase::Deferred, HookVariant::Html);
        let mut ctx = tera::Context::new();
        ctx.insert_value("data", d.data.clone());
        ctx.insert_value("site", generation.sites[lang].clone());
        ctx.insert_value("build", self.build_info.clone());
        ctx.insert_value(SCOPE_KEY, scope.to_value());
        self.templates
            .tera()
            .render(&d.template, &ctx)
            .map_err(|source| RenderError::Deferred {
                key: key.to_owned(),
                template: d.template.to_string(),
                source: Box::new(source),
            })
    }

    /// Runs a content adapter: renders `source`, the Tera template of the `_content.html` at
    /// `path`, for language `lang` as run `run` of [`Handles::adapters`] (which collects what
    /// it adds); the output is discarded. The context has `site` (the language's, without the
    /// page lists: Go's site is not built yet either), `build`, `lang` and a scope in phase
    /// `Adapter` on the language's home page, so site functions and partials work on the
    /// session's model (the content files).
    ///
    /// # Errors
    /// [`RenderError::Adapter`] when the template does not parse or fails.
    pub fn render_adapter(
        &self,
        path: &str,
        source: &str,
        lang: LangIdx,
        run: u32,
    ) -> Result<(), RenderError> {
        let home = self.model.sites[lang].home;
        let scope = RenderScope {
            phase: Phase::Adapter,
            adapter: Some(run),
            ..RenderScope::layout(home, lang, self.html_format, None)
        };
        let generation = self.views.generation(Phase::Adapter, HookVariant::Html);
        let site: tera::Map = generation.sites[lang]
            .clone()
            .into_map()
            .unwrap_or_default()
            .into_iter()
            .filter(|(k, _)| {
                !k.as_str()
                    .is_some_and(|k| ADAPTER_HIDDEN_SITE_KEYS.contains(&k))
            })
            .collect();
        let mut ctx = tera::Context::new();
        ctx.insert_value("site", tera::Value::from(site));
        ctx.insert_value("build", self.build_info.clone());
        ctx.insert("lang", &self.model.config.sites[lang].language.key);
        ctx.insert_value(SCOPE_KEY, scope.to_value());
        self.templates
            .tera()
            .render_str(source, &ctx, false)
            .map(drop)
            .map_err(|source| RenderError::Adapter {
                path: path.to_owned(),
                source: Box::new(source),
            })
    }

    fn permalink(&self, page: PageId, format: FormatId) -> Result<String, RenderError> {
        outputs(&self.model.pages[page])
            .find(|o| o.format == format)
            .and_then(|o| o.links.as_ref())
            .map(|l| l.permalink.to_string())
            .ok_or(RenderError::NoOutput { page, format })
    }

    /// The order of a job.
    #[must_use]
    pub fn order(&self, job: &Job) -> JobOrder {
        let rank = |f: FormatId| u8::try_from(f.index()).unwrap_or(u8::MAX);
        let of = |page: PageId, format: FormatId, sub: u8| {
            JobOrder::new(self.model.pages[page].lang, rank(format), page.raw(), sub)
        };
        match *job {
            Job::Alias(ref a) => of(a.to, a.format, 0),
            Job::Page { page, format } => of(page, format, 1),
            Job::PagerAlias { page, format } => of(page, format, 2),
            Job::Pager { page, format, .. } => of(page, format, 3),
            Job::Standalone { page, format } => of(page, format, 4),
            Job::LanguageRedirect => JobOrder::new(LangIdx::from_index(0), 0, 0, 5),
        }
    }

    fn render_layout(
        &self,
        job: &Job,
        page: PageId,
        format: FormatId,
        pager: Option<u32>,
        target: OutputPath,
    ) -> Result<Vec<Output>, RenderError> {
        let Some(sel) = self.selections.get(&(page, format)) else {
            return Ok(Vec::new());
        };
        let cfg = &self.model.config;
        let p = &self.model.pages[page];
        let mut scope = RenderScope::layout(page, p.lang, format, pager);
        // A layout job for format F sees variant `Format(F)` if it exists, else `Html`.
        scope.variant = self.known_variant(HookVariant::Format(format));
        let generation = self.views.generation(Phase::Layout, scope.variant);
        let full = generation.page_value(page);
        let format_name = &cfg.output_formats.get(format).name;
        let output_format = full
            .as_map()
            .and_then(|m| m.get(&tera::value::Key::Str("output_formats")))
            .and_then(tera::Value::as_map)
            .and_then(|m| m.get(&tera::value::Key::String(format_name.clone().into())))
            .cloned()
            .unwrap_or_else(tera::Value::none);
        let mut ctx = tera::Context::new();
        ctx.insert_value("page", full);
        ctx.insert_value("site", generation.sites[p.lang].clone());
        ctx.insert_value("build", self.build_info.clone());
        ctx.insert("lang", &cfg.sites[p.lang].language.key);
        ctx.insert_value("output_format", output_format);
        if p.kind == PageKind::SitemapIndex {
            ctx.insert_value("sites", tera::Value::from(generation.sites.as_slice()));
        }
        ctx.insert_value(SCOPE_KEY, scope.to_value());
        let text = self
            .templates
            .tera()
            .render(sel.render_as.as_str(), &ctx)
            .map_err(|source| RenderError::Render {
                template: sel.render_as.to_string(),
                page: p.key.to_path(),
                source: Box::new(source),
            })?;
        Ok(vec![Output {
            path: target,
            text,
            format,
            lang: p.lang,
            is_html: cfg.output_formats.get(format).is_html,
            order: self.order(job),
        }])
    }

    /// The alias template (user, theme or embedded `alias.html`).
    fn alias_template(&self) -> Option<TemplateName> {
        self.project
            .layouts
            .templates()
            .filter(|t| t.role == TemplateRole::Standalone(StandaloneKind::Alias))
            .min_by(|a, b| a.origin.cmp(&b.origin))
            .map(|t| t.render_name().clone())
    }

    fn render_alias(
        &self,
        job: &Job,
        from: &OutputPath,
        to: &str,
        page: Option<PageId>,
        format: FormatId,
    ) -> Result<Vec<Output>, RenderError> {
        let Some(name) = self.alias_template() else {
            return Ok(Vec::new());
        };
        let lang = page.map_or(LangIdx::from_index(0), |p| self.model.pages[p].lang);
        let generation = self.views.generation(Phase::Layout, HookVariant::Html);
        let mut ctx = tera::Context::new();
        ctx.insert("permalink", to);
        ctx.insert_value(
            "page",
            page.map_or_else(tera::Value::none, |p| generation.links[p].clone()),
        );
        ctx.insert_value("site", generation.sites[lang].clone());
        ctx.insert_value("build", self.build_info.clone());
        let text = self
            .templates
            .tera()
            .render(name.as_str(), &ctx)
            .map_err(|source| RenderError::Render {
                template: name.to_string(),
                page: from.to_string(),
                source: Box::new(source),
            })?;
        Ok(vec![Output {
            path: from.clone(),
            text,
            format,
            lang,
            is_html: true,
            order: self.order(job),
        }])
    }

    /// Wave 1 of language `lang` (phase E2):
    /// aliases, then every page × format, then the standalone pages (robots.txt and the
    /// sitemap index with the first language), in [`JobOrder`].
    #[must_use]
    pub fn wave1(&self, lang: LangIdx) -> Vec<Job> {
        let model = &self.model;
        let mut jobs: Vec<Job> = self.aliases[lang].iter().cloned().map(Job::Alias).collect();
        // robots.txt and the sitemap index are rendered once, with the first language.
        let root = |p: &Page| {
            matches!(p.kind, PageKind::RobotsTxt | PageKind::SitemapIndex) && p.lang.index() == 0
        };
        let mut standalone: Vec<PageId> = model
            .pages
            .iter()
            .filter(|p| p.lang == lang && p.standalone.is_some() && !root(p))
            .map(|p| p.id)
            .collect();
        if lang.index() == 0 {
            standalone.extend(model.pages.iter().filter(|p| root(p)).map(|p| p.id));
        }
        for p in &model.pages {
            if p.lang != lang || is_standalone(p.kind) {
                continue;
            }
            for o in outputs(p) {
                jobs.push(Job::Page {
                    page: p.id,
                    format: o.format,
                });
            }
        }
        for s in standalone {
            for o in outputs(&model.pages[s]) {
                jobs.push(Job::Standalone {
                    page: s,
                    format: o.format,
                });
            }
        }
        jobs.sort_by_key(|j| self.order(j));
        jobs
    }

    /// Wave 2 (phase E3): from the paginations recorded in wave 1, the `page/1/` aliases (HTML
    /// formats, unless `pagination.disableAliases`) and pagers 2..N; then the language
    /// redirect.
    #[must_use]
    pub fn wave2(&self) -> Vec<Job> {
        let cfg = &self.model.config;
        let mut jobs = Vec::new();
        for ((page, format), rec) in self.pagination.recorded() {
            let site = &cfg.sites[self.model.pages[page].lang];
            if cfg.output_formats.get(format).is_html && !site.pagination.disable_aliases {
                jobs.push(Job::PagerAlias { page, format });
            }
            for number in 2..=rec.total_pages() {
                jobs.push(Job::Pager {
                    page,
                    format,
                    number,
                });
            }
        }
        jobs.sort_by_key(|j| self.order(j));
        if self.language_redirect().is_some() {
            jobs.push(Job::LanguageRedirect);
        }
        jobs
    }

    /// The layout chosen for page `page` in `format` in [`new`](Self::new) (the template and
    /// base template its [`Job::Page`] or [`Job::Standalone`] renders); `None`: no layout, the
    /// job renders nothing. The structure dump of `ssg-build` records it.
    #[must_use]
    pub fn selection(&self, page: PageId, format: FormatId) -> Option<&Selection> {
        self.selections.get(&(page, format))
    }

    /// The redirect to the default language's home page (`ssg_nav::language_redirect`:
    /// `/en/` → `/`, or `/` → `/en/` with `defaultContentLanguageInSubdir`), when the home page
    /// has that output: what [`Job::LanguageRedirect`] renders.
    #[must_use]
    pub fn language_redirect(&self) -> Option<AliasPlan> {
        let a = ssg_nav::language_redirect(self.views.nav(), &self.model.config)?;
        outputs(&self.model.pages[a.to])
            .any(|o| o.format == a.format)
            .then_some(a)
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
