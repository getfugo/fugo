//! A small bilingual site loaded into a model, its views, a resource store with assets and an
//! image queue, and one Tera instance with the pure and site-bound functions; a test render
//! session implements `ContentRenderer`.

use std::fs;
use std::path::Path;
use std::sync::{Arc, OnceLock, Weak};

use dashmap::DashMap;
use ssg_base::diag::Diagnostics;
use ssg_base::{Clock, FormatId, Idx, PageId, PageKind};
use ssg_config::LoadOptions;
use ssg_funcs::{PureEnv, register_pure};
use ssg_highlight::Highlight;
use ssg_images::{ImageQueue, Imaging};
use ssg_layouts::{LayoutStore, Selections, TemplateName, Templates};
use ssg_locale::TranslationsBuilder;
use ssg_markup::Fragments;
use ssg_resources::{ResourceStore, StoreConfig};
use ssg_site::{LoadModelOptions, Model};
use ssg_sitefuncs::{Frames, Handles, RelatedCache};
use ssg_vfs::Vfs;
use ssg_view::{
    ContentError, ContentRenderer, DeferredRegistry, ExpandedSource, HookVariant, NavSite,
    PageStores, PaginationRecorder, RenderScope, RenderStringOptions, RenderedContent, SCOPE_KEY,
    Stage, ViewCache, ViewInputs,
};

pub const FILES: &[(&str, &str)] = &[
    (
        "config.toml",
        r#"baseURL = "https://example.org/sub/"
title = "Funcs"
defaultContentLanguage = "en"
refLinksNotFoundURL = "/404-ref/"
[params]
color = "blue"
[params.nested]
deep = "site-deep"
[languages.en]
weight = 1
languageName = "English"
[languages.fr]
weight = 2
languageName = "Français"
[pagination]
pagerSize = 2
[taxonomies]
tag = "tags"
[[menus.main]]
name = "Posts"
pageRef = "/posts"
weight = 1
[[menus.main]]
name = "One"
parent = "Posts"
pageRef = "/posts/one"
[related]
threshold = 1
includeNewer = true
toLower = true
[[related.indices]]
name = "tags"
weight = 100
"#,
    ),
    ("content/_index.md", "---\ntitle: Home\n---\nWelcome.\n"),
    (
        "content/_index.fr.md",
        "---\ntitle: Accueil\n---\nBienvenue.\n",
    ),
    ("content/about.md", "---\ntitle: About\n---\nAbout.\n"),
    (
        "content/posts/_index.md",
        "---\ntitle: Posts\n---\nAll posts.\n",
    ),
    (
        "content/posts/one.md",
        "---\ntitle: One\ndate: 2021-02-01T10:00:00Z\nlastmod: 2021-06-01T00:00:00Z\ntags: [a, b]\nweight: 2\nparams:\n  color: red\n  nested:\n    deep: page-deep\n  series: x\n---\n# Hello\n\nOne body.\n",
    ),
    (
        "content/posts/one.fr.md",
        "---\ntitle: Un\ndate: 2021-02-01T10:00:00Z\n---\nCorps.\n",
    ),
    (
        "content/posts/two.md",
        "---\ntitle: Two\ndate: 2022-04-01T00:00:00Z\nlastmod: 2021-03-01T00:00:00Z\ntags: [a]\nweight: 1\nlinkTitle: Zwei\nparams:\n  series: z\n---\nTwo body.\n",
    ),
    (
        "content/posts/three.md",
        "---\ntitle: Three\ndate: 2022-05-01T00:00:00Z\nlastmod: 2023-01-01T00:00:00Z\ntags: [b]\nparams:\n  series: x\n---\nThree body.\n",
    ),
    (
        "content/posts/bundle/index.md",
        "---\ntitle: Bundle\ndate: 2020-01-01T00:00:00Z\n---\nBundle body.\n",
    ),
    (
        "content/posts/bundle/notes.md",
        "---\ntitle: Notes\n---\nNotes body.\n",
    ),
    ("content/posts/bundle/data.txt", "bundle data\n"),
    ("assets/css/a.css", "body { color: red; }\n"),
    ("assets/css/b.css", "p { margin: 0; }\n"),
    ("assets/data/x.json", "{\"b\": 2, \"a\": [1, 2]}"),
    ("assets/data/rows.csv", "a,b\n1,2\n"),
    (
        "assets/tpl/greet.txt",
        "Hello {{ data.name }} from {{ site.title }} ({{ __nh.page }})",
    ),
    // ── layouts: partials and components the tests call ──
    (
        "layouts/_partials/text.html",
        "<b>{{ word }}</b>|{{ page.title }}|{{ lang }}",
    ),
    (
        "layouts/_partials/inner.html",
        "{{ return_value(value={\"n\": n, \"from\": page.title}) }}ignored",
    ),
    (
        "layouts/_partials/outer.html",
        "{% set got = partial(name=\"inner\", n=n + 1) %}{{ return_value(value=[got.n, got.from, n]) }}",
    ),
    (
        "layouts/_partials/count.html",
        "{{ store_set(key=\"calls\", value=(store_get(key=\"calls\") or 0) + 1) }}{{ return_value(value={\"k\": k}) }}",
    ),
    (
        "layouts/_partials/pager.html",
        "{% set p = paginator() %}{{ p.page_number }}/{{ p.total_pages }}",
    ),
    (
        "layouts/_partials/paginate_other.html",
        "{% set p = paginate(pages=site.regular_pages, size=1) %}{{ p.page_number }}",
    ),
    ("layouts/_partials/deferred/late.html", "late {{ data.x }}"),
    (
        "layouts/_partials/components.html",
        "{% component tr(key, @page) %}{{ i18n(key=key, page=page) }}{% endcomponent tr %}\
         {% component pg(@__nh) %}{% set p = paginator() %}{{ p.page_number }}{% endcomponent pg %}\
         {% component bad(key) %}{{ i18n(key=key) }}{% endcomponent bad %}",
    ),
];

/// The i18n messages of the site.
const I18N_EN: &str = "[hello]\nother = \"Hello\"\n[posts]\none = \"{{ .Count }} post\"\nother = \"{{ .Count }} posts\"\n";
const I18N_FR: &str = "[hello]\nother = \"Bonjour\"\n";

/// A synthetic render session: content is `<p>content of <title></p>`, Markdown is wrapped
/// in a marker, templates render with the loaded Tera instance.
pub struct TestRenderer {
    pub model: Arc<Model>,
    pub templates: Arc<Templates>,
}

fn check_cycle(s: &RenderScope, key: (PageId, Stage)) -> Result<(), ContentError> {
    if s.chain.contains(&key) {
        return Err(ContentError::Cycle(format!("{key:?}")));
    }
    Ok(())
}

impl ContentRenderer for TestRenderer {
    fn content(
        &self,
        p: PageId,
        v: HookVariant,
        s: &RenderScope,
    ) -> Result<Arc<RenderedContent>, ContentError> {
        check_cycle(s, (p, Stage::Content(v)))?;
        let title = &self.model.pages[p].title;
        Ok(Arc::new(RenderedContent {
            html: format!("<p>content of {title}</p>"),
            summary: format!("summary of {title}"),
            plain: format!("plain {title}"),
            word_count: 3,
            table_of_contents: "<nav></nav>".to_owned(),
            ..RenderedContent::default()
        }))
    }

    fn fragments(&self, p: PageId, s: &RenderScope) -> Result<Arc<Fragments>, ContentError> {
        check_cycle(s, (p, Stage::Fragments))?;
        Ok(Arc::new(Fragments {
            headings: Vec::new(),
            identifiers: vec!["hello".to_owned()],
        }))
    }

    fn render_shortcodes(
        &self,
        p: PageId,
        s: &RenderScope,
    ) -> Result<Arc<ExpandedSource>, ContentError> {
        check_cycle(s, (p, Stage::Expand))?;
        Ok(Arc::new(ExpandedSource {
            markdown: "before NHSC0X after NHSC1X".to_owned(),
            placeholders: vec![Arc::from("<b>zero</b>"), Arc::from("<i>one</i>")],
            ..ExpandedSource::default()
        }))
    }

    fn render_markdown(
        &self,
        md: &str,
        o: RenderStringOptions,
        s: &RenderScope,
    ) -> Result<String, ContentError> {
        let title = &self.model.pages[s.page].title;
        Ok(if o.display_block {
            format!("<p>{md}</p><!-- {title} depth {} -->", s.depth)
        } else {
            format!("{md}<!-- {title} depth {} -->", s.depth)
        })
    }

    fn render_template(
        &self,
        t: &TemplateName,
        ctx: tera::Context,
        s: &RenderScope,
    ) -> Result<String, ContentError> {
        let mut ctx = ctx;
        ctx.insert_value(SCOPE_KEY, s.to_value());
        self.templates
            .tera()
            .render(t.as_str(), &ctx)
            .map_err(|e| ContentError::Render(e.to_string()))
    }
}

/// A loaded site with its functions registered.
pub struct Site {
    pub _dir: tempfile::TempDir,
    pub model: Arc<Model>,
    pub views: Arc<ViewCache>,
    pub store: Arc<ResourceStore>,
    pub handles: Handles,
    pub templates: Arc<Templates>,
    /// Keeps the session alive: the functions hold it weakly.
    pub _renderer: Arc<TestRenderer>,
}

pub fn load() -> Site {
    load_with(&[])
}

/// The site of [`FILES`] plus `extra` files.
pub fn load_with(extra: &[(&str, &str)]) -> Site {
    load_copying(extra, &[])
}

/// The site of [`FILES`] plus `extra` files and copies of files (`(site path, source)`).
pub fn load_copying(extra: &[(&str, &str)], copies: &[(&str, &Path)]) -> Site {
    let dir = tempfile::tempdir().expect("tempdir");
    for (rel, text) in FILES.iter().chain(extra) {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    }
    for (rel, from) in copies {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::copy(from, path).expect("copy");
    }
    let png = ssg_testkit::fixture::testdata("site-assets/generated/square-128.png");
    fs::copy(&png, dir.path().join("content/posts/bundle/pic.png")).expect("copy png");
    fs::create_dir_all(dir.path().join("assets/img")).expect("mkdir");
    fs::copy(&png, dir.path().join("assets/img/logo.png")).expect("copy png");
    build(dir)
}

fn build(dir: tempfile::TempDir) -> Site {
    let cfg = Arc::new(
        ssg_config::load(&LoadOptions {
            source: dir.path().to_path_buf(),
            config_files: Vec::new(),
            cli: ssg_config::CliOverrides::default(),
            env: Vec::new(),
        })
        .expect("config"),
    );
    let vfs = Arc::new(Vfs::new(&cfg).expect("vfs"));
    let clock = Clock("2026-01-01T00:00:00Z".parse().expect("clock"));
    let model = Arc::new(
        ssg_site::load_model(
            Arc::clone(&cfg),
            &vfs,
            &LoadModelOptions::from_config(&cfg, clock),
        )
        .expect("model"),
    );
    let images = Arc::new(ImageQueue::new(Imaging::default(), None));
    let mut sc = StoreConfig::from_config(&cfg, Some(Arc::clone(&vfs)), Some(Arc::clone(&images)));
    sc.remote.network = false;
    sc.remote.cache_dir = None;
    let store = Arc::new(ResourceStore::new(sc));
    let (menus, _) = ssg_nav::build_menus(&NavSite::new(Arc::clone(&model)), &cfg);
    let menus = Arc::new(menus);
    let views = Arc::new(
        ViewCache::new(ViewInputs {
            model: Arc::clone(&model),
            store: Arc::clone(&store),
            menus: Arc::clone(&menus),
        })
        .expect("views"),
    );
    let mut tb = TranslationsBuilder::new("en");
    tb.add_file(Path::new("i18n/en.toml"), I18N_EN).expect("en");
    tb.add_file(Path::new("i18n/fr.toml"), I18N_FR).expect("fr");
    let i18n = Arc::new(tb.build(cfg.sites.iter().map(|s| s.language.key.as_str())));
    let handles = Handles {
        model: Arc::clone(&model),
        views: Arc::clone(&views),
        store: Arc::clone(&store),
        images,
        stores: Arc::new(PageStores::new(model.pages.len())),
        pagination: Arc::new(PaginationRecorder::default()),
        deferred: Arc::new(DeferredRegistry::default()),
        css_purges: Arc::default(),
        menus,
        related: Arc::new(RelatedCache::default()),
        i18n,
        diagnostics: Arc::new(Diagnostics::default()),
        highlight: Arc::new(Highlight::new(&cfg.default_site().markup.highlight)),
        renderer: Arc::new(OnceLock::new()),
        templates: Arc::new(OnceLock::new()),
        frames: Arc::new(Frames::default()),
        partial_cache: Arc::new(DashMap::new()),
        adapters: Arc::default(),
    };
    let layouts = Arc::new(LayoutStore::scan(&vfs, &cfg).expect("layouts"));
    let pure = Arc::new(PureEnv::new("en"));
    let templates = Arc::new(
        ssg_layouts::load(layouts, &Selections::new(), &|t| {
            register_pure(t, &pure);
            ssg_sitefuncs::register(t, &handles);
        })
        .expect("templates"),
    );
    let _ = handles.templates.set(Arc::downgrade(&templates));
    let renderer = Arc::new(TestRenderer {
        model: Arc::clone(&model),
        templates: Arc::clone(&templates),
    });
    let weak: Weak<dyn ContentRenderer> =
        Arc::downgrade(&(Arc::clone(&renderer) as Arc<dyn ContentRenderer>));
    let _ = handles.renderer.set(weak);
    Site {
        _dir: dir,
        model,
        views,
        store,
        handles,
        templates,
        _renderer: renderer,
    }
}

impl Site {
    /// The id of the page of `kind` at `.Path` `path` in language index `lang`.
    pub fn page(&self, kind: PageKind, path: &str, lang: usize) -> PageId {
        self.model
            .pages
            .iter()
            .find(|p| p.kind == kind && p.path() == path && p.lang.index() == lang)
            .unwrap_or_else(|| panic!("no {kind:?} page {path} ({lang})"))
            .id
    }

    pub fn html(&self) -> FormatId {
        self.model
            .config
            .output_formats
            .by_name("html")
            .expect("html")
    }

    /// The layout scope of `page` in HTML (pager `pager`).
    pub fn scope(&self, page: PageId, pager: Option<u32>) -> RenderScope {
        RenderScope::layout(page, self.model.pages[page].lang, self.html(), pager)
    }

    /// The context of a layout render of `s`.
    pub fn context(&self, s: &RenderScope) -> tera::Context {
        let g = self.views.generation(s.phase, s.variant);
        let mut ctx = tera::Context::new();
        ctx.insert_value("page", g.full(s.page));
        ctx.insert_value("site", g.sites[s.lang].clone());
        ctx.insert_value(
            "lang",
            tera::Value::from(self.model.config.sites[s.lang].language.key.as_str()),
        );
        ctx.insert_value(SCOPE_KEY, s.to_value());
        ctx
    }

    /// Renders `source` in the layout of `s`.
    pub fn try_render(&self, source: &str, s: &RenderScope) -> Result<String, tera::Error> {
        self.templates
            .tera()
            .render_str(source, &self.context(s), true)
    }

    pub fn render(&self, source: &str, s: &RenderScope) -> String {
        self.try_render(source, s)
            .unwrap_or_else(|e| panic!("render {source:?}: {e}"))
    }

    /// Renders `source` without a render scope (only `lang` in the context).
    pub fn render_bare(&self, source: &str) -> Result<String, tera::Error> {
        let mut ctx = tera::Context::new();
        ctx.insert_value("lang", tera::Value::from("en"));
        self.templates.tera().render_str(source, &ctx, true)
    }

    /// The home page's layout scope.
    pub fn home_scope(&self) -> RenderScope {
        self.scope(self.page(PageKind::Home, "/", 0), None)
    }
}
