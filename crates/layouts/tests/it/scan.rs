//! The scan: v0.146 names, legacy names refused with the name to use, Go-template files
//! refused, themes and their prefixes, roles and descriptors.

use std::fs;
use std::path::PathBuf;

use ssg_base::PageKind;
use ssg_base::paths::ContentKey;
use ssg_config::output::Escaping;
use ssg_config::{Config, LoadOptions, load};
use ssg_layouts::{
    HookKind, IssueKind, LayoutEnv, LayoutQuery, LayoutSource, LayoutStore, Origin, StandaloneKind,
    TemplateError, TemplateRole,
};
use ssg_vfs::Vfs;

/// A project on disk.
pub struct Project {
    _tmp: tempfile::TempDir,
    pub dir: PathBuf,
}

impl Project {
    pub fn new(files: &[(&str, &str)]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("site");
        let theme = [("themes/mytheme/theme.toml", "")];
        for (name, content) in files.iter().chain(&theme) {
            let p = dir.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, content).unwrap();
        }
        Self { _tmp: tmp, dir }
    }

    pub fn config(&self) -> Config {
        load(&LoadOptions {
            source: self.dir.clone(),
            env: vec![(
                "HOME".into(),
                self.dir.join("_home").to_str().unwrap().into(),
            )],
            ..LoadOptions::default()
        })
        .unwrap()
    }

    pub fn scan(&self) -> Result<LayoutStore, TemplateError> {
        let cfg = self.config();
        LayoutStore::scan(&Vfs::new(&cfg).unwrap(), &cfg)
    }
}

const CONFIG: &str = r#"
baseURL = "https://example.org/"
theme = ["mytheme"]
defaultContentLanguage = "en"
[languages.en]
weight = 1
[languages.th]
weight = 2
[outputFormats.plainxml]
mediaType = "application/xml"
isPlainText = true
[mediaTypes."text/netlify"]
suffixes = ["redir"]
[outputFormats.redir]
mediaType = "text/netlify"
isPlainText = true
baseName = "_redirects"
"#;

fn issues(e: TemplateError) -> Vec<(String, IssueKind)> {
    match e {
        TemplateError::Layouts(v) => v
            .into_iter()
            .map(|i| {
                let f = i.position.file.to_string_lossy().into_owned();
                let rel = f.split("/layouts/").nth(1).unwrap_or(&f).to_owned();
                (rel, i.kind)
            })
            .collect(),
        other => panic!("{other}"),
    }
}

fn legacy(to: &str) -> IssueKind {
    IssueKind::LegacyName {
        rename: to.to_owned(),
    }
}

#[test]
fn legacy_names_are_refused_with_the_new_name() {
    let p = Project::new(&[
        ("config.toml", CONFIG),
        ("layouts/_default/single.html", "x"),
        ("layouts/_default/_markup/render-link.html", "x"),
        ("layouts/partials/head/meta.html", "x"),
        ("layouts/shortcodes/note.html", "x"),
        ("layouts/docs/list-baseof.html", "x"),
        ("layouts/taxonomy/list.html", "x"),
        ("layouts/term/term.html", "x"),
        ("layouts/index.html", "x"),
        ("layouts/index.json", "x"),
        ("layouts/single.html", "fine"),
        ("themes/mytheme/layouts/_default/list.html", "x"),
    ]);
    let got = issues(p.scan().unwrap_err());
    let want = vec![
        (
            "_default/_markup/render-link.html".to_owned(),
            legacy("_markup/render-link.html"),
        ),
        ("_default/single.html".to_owned(), legacy("single.html")),
        (
            "docs/list-baseof.html".to_owned(),
            legacy("docs/baseof.list.html"),
        ),
        ("index.html".to_owned(), legacy("home.html")),
        ("index.json".to_owned(), legacy("home.json")),
        (
            "partials/head/meta.html".to_owned(),
            legacy("_partials/head/meta.html"),
        ),
        (
            "shortcodes/note.html".to_owned(),
            legacy("_shortcodes/note.html"),
        ),
        ("taxonomy/list.html".to_owned(), legacy("term.html")),
        ("term/term.html".to_owned(), legacy("term.html")),
        ("_default/list.html".to_owned(), legacy("list.html")),
    ];
    let mut want = want;
    want.sort_by(|a, b| a.0.cmp(&b.0));
    let mut got_sorted = got.clone();
    got_sorted.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(got_sorted, want);
    let msg = TemplateError::Layouts(Vec::new()).to_string();
    assert!(msg.contains("0 layout file(s)"), "{msg}");
}

#[test]
fn go_templates_are_refused_with_their_line() {
    let p = Project::new(&[
        ("config.toml", CONFIG),
        ("layouts/single.html", "<p>\n{{ .Title }}</p>"),
        ("layouts/list.html", "{{ define \"main\" }}x{% endblock %}"),
        (
            "layouts/home.html",
            "{% raw %}{{ .Title }}{% endraw %}{{ page.title }}",
        ),
        ("layouts/_partials/a.html", "{{- range $i, $p := x }}"),
        ("layouts/_partials/b.html", "{{ x }}{{ end }}"),
        ("layouts/_partials/c.html", "{{/* c */}}"),
        (
            "layouts/_partials/ok.html",
            "{{ range(end=3) | join }}{{ if_x }}{# {{ .x }} #}",
        ),
    ]);
    let got = issues(p.scan().unwrap_err());
    let markers: Vec<(String, String)> = got
        .into_iter()
        .map(|(f, k)| match k {
            IssueKind::GoTemplate { marker } => (f, marker),
            other => panic!("{f}: {other:?}"),
        })
        .collect();
    assert_eq!(
        markers,
        vec![
            ("_partials/a.html".to_owned(), "{{ range".to_owned()),
            ("_partials/b.html".to_owned(), "{{ end".to_owned()),
            ("_partials/c.html".to_owned(), "{{/*".to_owned()),
            ("list.html".to_owned(), "{{ define".to_owned()),
            ("single.html".to_owned(), "{{ .".to_owned()),
        ]
    );
    let err = p.scan().unwrap_err().to_string();
    assert!(err.contains("single.html:2"), "{err}");
    assert!(err.contains("templates check"), "{err}");
}

#[test]
fn unknown_names_are_refused() {
    let p = Project::new(&[
        ("config.toml", CONFIG),
        ("layouts/_markup/link.html", "x"),
        ("layouts/_markup/render-footnote.html", "x"),
        ("layouts/_internal/x.html", "x"),
        ("layouts/notes.unknownsuffix", "x"),
        ("layouts/.hidden.html", "{{ .x }}"),
        ("layouts/backup.html~", "{{ .x }}"),
    ]);
    let got = issues(p.scan().unwrap_err());
    let files: Vec<&str> = got.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(
        files,
        [
            "_internal/x.html",
            "_markup/link.html",
            "_markup/render-footnote.html",
            "notes.unknownsuffix"
        ]
    );
    assert!(
        got.iter()
            .all(|(_, k)| matches!(k, IssueKind::UnknownName { .. }))
    );
}

#[test]
fn roles_descriptors_and_theme_prefixes() {
    let p = Project::new(&[
        ("config.toml", CONFIG),
        ("layouts/baseof.html", "{% block main %}{% endblock %}"),
        ("layouts/docs/baseof.list.html", "x"),
        ("layouts/home.th.html", "x"),
        ("layouts/list.rss.xml", "x"),
        ("layouts/404.html", "x"),
        ("layouts/robots.txt", "x"),
        ("layouts/rss.xml", "x"),
        ("layouts/home.redir", "x"),
        ("layouts/posts/Special.HTML", "x"),
        ("layouts/_partials/helpers/Picture.html", "x"),
        ("layouts/blog/_shortcodes/note.th.html", "x"),
        ("layouts/docs/_markup/render-codeblock-go.html", "x"),
        ("layouts/feed.plainxml.xml", "x"),
        ("themes/mytheme/layouts/single.html", "theme"),
        (
            "themes/mytheme/layouts/list.rss.xml",
            "hidden by the project's",
        ),
    ]);
    let store = p.scan().unwrap();
    let env = store.env();
    let fmt = |n: &str| env.formats().by_name(n).unwrap();
    let get = |n: &str| {
        store
            .templates()
            .find(|t| t.name.as_str() == n)
            .unwrap_or_else(|| panic!("{n}"))
    };
    let th = env.parser().language("th").unwrap();

    assert_eq!(
        get("home.th.html").role,
        TemplateRole::Layout {
            kind: Some(PageKind::Home),
            layout: None
        }
    );
    assert_eq!(get("home.th.html").lang, Some(th));
    assert_eq!(
        get("docs/baseof.list.html").role,
        TemplateRole::Base {
            kind: None,
            layout: Some("list".into())
        }
    );
    assert_eq!(
        get("docs/baseof.list.html").scope,
        ContentKey::from_source("docs")
    );
    assert_eq!(get("list.rss.xml").format, Some(fmt("rss")));
    assert_eq!(
        get("404.html").role,
        TemplateRole::Standalone(StandaloneKind::NotFound)
    );
    assert_eq!(
        get("robots.txt").role,
        TemplateRole::Standalone(StandaloneKind::RobotsTxt)
    );
    assert_eq!(get("robots.txt").escaping, Escaping::Plain);
    assert_eq!(
        get("rss.xml").role,
        TemplateRole::Layout {
            kind: None,
            layout: None
        }
    );
    assert_eq!(get("home.redir").format, Some(fmt("redir")));
    assert_eq!(get("home.redir").escaping, Escaping::Plain);
    assert_eq!(
        get("posts/special.html").role,
        TemplateRole::Layout {
            kind: None,
            layout: Some("special".into())
        },
        "names are lower-cased"
    );
    assert_eq!(
        get("_partials/helpers/picture.html").role,
        TemplateRole::Partial {
            name: "helpers/picture".into()
        }
    );
    let note = get("blog/_shortcodes/note.th.html");
    assert_eq!(
        note.role,
        TemplateRole::Shortcode {
            name: "note".into()
        }
    );
    assert_eq!(note.scope, ContentKey::from_source("blog"));
    assert_eq!(note.lang, Some(th));
    let hook = get("docs/_markup/render-codeblock-go.html");
    assert_eq!(
        hook.role,
        TemplateRole::Hook {
            kind: HookKind::CodeBlock,
            variant: Some("go".into())
        }
    );
    assert_eq!(hook.scope, ContentKey::from_source("docs"));
    // A plain-text format whose file suffix Tera would escape renders under an alias.
    let sm = get("feed.plainxml.xml");
    assert_eq!(sm.escaping, Escaping::Plain);
    assert_eq!(sm.render_name().as_str(), "feed.plainxml.xml@@plain");
    assert_eq!(get("home.redir").render_name().as_str(), "home.redir");

    let theme = get("_theme1/single.html");
    assert!(matches!(theme.origin, Origin::Theme(1, _)));
    assert!(
        store
            .templates()
            .all(|t| t.name.as_str() != "_theme1/list.rss.xml"),
        "the project's file hides the theme's of the same path"
    );
    // The theme's single.html serves pages.
    let key = ContentKey::from_source("posts/p1");
    let sel = store
        .select(&LayoutQuery {
            path: &key,
            kind: Some(PageKind::Page),
            layout: None,
            exact_layout: false,
            lang: None,
            format: fmt("html"),
        })
        .unwrap();
    assert_eq!(sel.layout.as_str(), "_theme1/single.html");
    assert_eq!(sel.base, None, "the layout does not extend baseof.html");
    // A front-matter layout.
    let sel = store
        .select(&LayoutQuery {
            path: &key,
            kind: Some(PageKind::Page),
            layout: Some("Special"),
            exact_layout: false,
            lang: None,
            format: fmt("html"),
        })
        .unwrap();
    assert_eq!(sel.layout.as_str(), "posts/special.html");
}

#[test]
fn from_sources_without_a_project() {
    let p = Project::new(&[("config.toml", CONFIG)]);
    let env = LayoutEnv::from_config(&p.config());
    let store = LayoutStore::from_sources(
        env,
        [
            LayoutSource {
                rel: "single.html".into(),
                origin: Origin::User("/p/layouts/single.html".into()),
                source: "u".into(),
            },
            LayoutSource {
                rel: "single.html".into(),
                origin: Origin::Embedded,
                source: "e".into(),
            },
        ],
    )
    .unwrap();
    let names: Vec<&str> = store.templates().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["single.html", "_embedded/single.html"]);
}
