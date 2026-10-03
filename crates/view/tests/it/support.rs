//! A small bilingual site with every page kind, loaded into a model and its views.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use ssg_base::{Clock, IdVec, PageId, PageKind};
use ssg_config::LoadOptions;
use ssg_resources::{ResourceStore, StoreConfig};
use ssg_site::{LoadModelOptions, Model, PageRole};
use ssg_vfs::Vfs;
use ssg_view::{Contents, HookVariant, NavSite, RenderedContent, ViewCache, ViewInputs};

pub const FILES: &[(&str, &str)] = &[
    (
        "config.toml",
        r#"baseURL = "https://example.org/"
title = "Views"
copyright = "(c) Views"
defaultContentLanguage = "en"
enableRobotsTXT = true
[params]
description = "A site"
[languages.en]
weight = 1
languageName = "English"
[languages.fr]
weight = 2
languageName = "Français"
[outputs]
home = ["html", "rss", "json", "headers"]
[outputFormats.headers]
baseName = "_headers"
isPlainText = true
mediaType = "text/plain"
notAlternative = true
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
pre = "<i>"
[privacy.youtube]
privacyEnhanced = true
[privacy.x]
enableDNT = true
[services.googleAnalytics]
ID = "G-1"
[services.rss]
limit = 10
"#,
    ),
    ("data/team.toml", "Lead = \"Ann\"\n"),
    ("content/_index.md", "---\ntitle: Home\n---\nWelcome.\n"),
    (
        "content/_index.fr.md",
        "---\ntitle: Accueil\n---\nBienvenue.\n",
    ),
    (
        "content/posts/_index.md",
        "---\ntitle: Posts\ndate: 2021-01-01\n---\nAll posts.\n",
    ),
    (
        "content/posts/one.md",
        "---\ntitle: One\ndate: 2021-02-01T10:00:00Z\nlastmod: 2021-03-01T10:00:00Z\ndescription: The first\ntags: [a, b]\naliases: [/old/one/]\nkeywords: [k1]\nparams:\n  color: red\n---\n# Hello\n\nOne *body*.\n",
    ),
    (
        "content/posts/one.fr.md",
        "---\ntitle: Un\ndate: 2021-02-01T10:00:00Z\ntags: [a]\n---\nCorps.\n",
    ),
    (
        "content/posts/two.md",
        "---\ntitle: Two\ndate: 2021-04-01\ntags: [a]\nlinkTitle: Second\n---\nTwo body.\n",
    ),
    (
        "content/posts/bundle/index.md",
        "---\ntitle: Bundle\ndate: 2021-05-01\nresources:\n- src: img.png\n  name: cover\n  title: Cover\n  params:\n    credit: me\n---\nBundle body.\n",
    ),
    ("content/posts/bundle/img.png", "not really a png"),
    ("content/posts/bundle/data.txt", "some data\n"),
    (
        "content/posts/bundle/notes.md",
        "---\ntitle: Notes\n---\nNotes body.\n",
    ),
];

/// A loaded site and its views (Meta generation only until [`freeze`] is called).
pub struct Site {
    pub _dir: tempfile::TempDir,
    pub model: Arc<Model>,
    pub store: Arc<ResourceStore>,
    pub views: ViewCache,
}

/// Loads `files` from a fresh directory.
pub fn load(files: &[(&str, &str)]) -> Site {
    let dir = tempfile::tempdir().expect("tempdir");
    for (rel, text) in files {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    }
    let (model, store, views) = load_dir(dir.path());
    Site {
        _dir: dir,
        model,
        store,
        views,
    }
}

/// Loads the site in `dir`: model, store, menus, Meta views.
pub fn load_dir(dir: &Path) -> (Arc<Model>, Arc<ResourceStore>, ViewCache) {
    let model = Arc::new(load_model(dir));
    let (store, views) = views_of(&model);
    (model, store, views)
}

/// The model of the site in `dir` (clock 2026-01-01).
pub fn load_model(dir: &Path) -> Model {
    let cfg = Arc::new(
        ssg_config::load(&LoadOptions {
            source: dir.to_path_buf(),
            config_files: Vec::new(),
            cli: ssg_config::CliOverrides::default(),
            env: Vec::new(),
        })
        .expect("config"),
    );
    let vfs = Vfs::new(&cfg).expect("vfs");
    let clock = Clock("2026-01-01T00:00:00Z".parse().expect("clock"));
    ssg_site::load_model(
        Arc::clone(&cfg),
        &vfs,
        &LoadModelOptions::from_config(&cfg, clock),
    )
    .expect("model")
}

/// The store and Meta views of `model`.
pub fn views_of(model: &Arc<Model>) -> (Arc<ResourceStore>, ViewCache) {
    let store = Arc::new(ResourceStore::new(StoreConfig::from_config(
        &model.config,
        None,
        None,
    )));
    let (menus, _) = ssg_nav::build_menus(&NavSite::new(Arc::clone(model)), &model.config);
    let views = ViewCache::new(ViewInputs {
        model: Arc::clone(model),
        store: Arc::clone(&store),
        menus: Arc::new(menus),
    })
    .expect("views");
    (store, views)
}

/// Whether page `p` has content of its own (a rendered page with a content file).
pub fn has_content(m: &Model, p: PageId) -> bool {
    let p = &m.pages[p];
    p.source.is_some() && p.role == PageRole::Standalone && p.rendered()
}

/// Synthetic content: `<p>{tag} {title}</p>`, so each variant is recognisable.
pub fn contents(m: &Model, tag: &str) -> Contents {
    m.pages
        .iter()
        .map(|p| {
            has_content(m, p.id).then(|| {
                Arc::new(RenderedContent {
                    html: format!("<p>{tag} {}</p>", p.title),
                    summary: format!("{tag} summary"),
                    plain: format!("{tag} {}", p.title),
                    word_count: 2,
                    fuzzy_word_count: 100,
                    reading_time: 1,
                    table_of_contents: "<nav id=\"TableOfContents\"></nav>".to_owned(),
                    ..RenderedContent::default()
                })
            })
        })
        .collect::<IdVec<PageId, _>>()
}

/// Freezes the Full generations `Html` and `Format(json)`.
pub fn freeze(s: &Site) -> HookVariant {
    let json = s
        .model
        .config
        .output_formats
        .by_name("json")
        .expect("json format");
    let mut all = BTreeMap::new();
    all.insert(HookVariant::Html, contents(&s.model, "html"));
    all.insert(HookVariant::Format(json), contents(&s.model, "json"));
    s.views.freeze(&all);
    HookVariant::Format(json)
}

/// The id of the page of `kind` at `.Path` `path` in language index `lang`.
pub fn page(m: &Model, kind: PageKind, path: &str, lang: usize) -> PageId {
    m.pages
        .iter()
        .find(|p| p.kind == kind && p.path() == path && ssg_base::Idx::index(p.lang) == lang)
        .unwrap_or_else(|| panic!("no {kind:?} page {path} ({lang})"))
        .id
}

/// The map entry `k` of a value.
pub fn get<'a>(v: &'a tera::Value, k: &str) -> &'a tera::Value {
    v.as_map()
        .and_then(|m| m.get(&tera::value::Key::String(k.into())))
        .unwrap_or_else(|| panic!("no key {k}"))
}

/// The keys of a map value.
pub fn keys(v: &tera::Value) -> Vec<String> {
    let mut k: Vec<String> = v
        .as_map()
        .expect("a map")
        .keys()
        .map(ToString::to_string)
        .collect();
    k.sort();
    k
}

/// Whether two values are the same allocation (Arc-shared maps or arrays).
pub fn same(a: &tera::Value, b: &tera::Value) -> bool {
    match (a.as_map(), b.as_map(), a.as_array(), b.as_array()) {
        (Some(x), Some(y), _, _) => std::ptr::eq(x, y),
        (_, _, Some(x), Some(y)) => std::ptr::eq(x.as_ptr(), y.as_ptr()),
        _ => false,
    }
}
