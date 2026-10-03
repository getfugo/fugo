//! Loading into Tera: fallback prefixes, escaping by output format, base variants,
//! `uses_variable`, load errors, and the converted site layouts under `sites/`.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use ssg_base::PageKind;
use ssg_base::paths::ContentKey;
use ssg_layouts::{
    LayoutEnv, LayoutQuery, LayoutSource, LayoutStore, Origin, Selections, TemplateError,
    Templates, load,
};

use crate::scan::Project;

const CONFIG: &str = r#"
baseURL = "https://example.org/"
[outputFormats.plainxml]
mediaType = "application/xml"
isPlainText = true
[outputFormats.htmlish]
mediaType = "text/plain"
isPlainText = false
# The docs site's Netlify formats (`sites/docs/layouts/home.{redir,headers}`).
[mediaTypes."text/netlify"]
delimiter = ""
[outputFormats.redir]
baseName = "_redirects"
isPlainText = true
mediaType = "text/netlify"
[outputFormats.headers]
baseName = "_headers"
isPlainText = true
mediaType = "text/netlify"
notAlternative = true
"#;

fn env() -> LayoutEnv {
    LayoutEnv::from_config(&Project::new(&[("config.toml", CONFIG)]).config())
}

fn user(rel: &str, source: &str) -> LayoutSource {
    LayoutSource {
        rel: rel.into(),
        origin: Origin::User(format!("/p/layouts/{rel}").into()),
        source: source.into(),
    }
}

fn theme(n: u8, rel: &str, source: &str) -> LayoutSource {
    LayoutSource {
        rel: rel.into(),
        origin: Origin::Theme(n, format!("/p/themes/t{n}/layouts/{rel}").into()),
        source: source.into(),
    }
}

fn embedded(rel: &str, source: &str) -> LayoutSource {
    LayoutSource {
        rel: rel.into(),
        origin: Origin::Embedded,
        source: source.into(),
    }
}

fn store(sources: Vec<LayoutSource>) -> Arc<LayoutStore> {
    Arc::new(LayoutStore::from_sources(env(), sources).unwrap())
}

fn loaded(sources: Vec<LayoutSource>, sel: &Selections) -> Templates {
    load(store(sources), sel, &|_| {}).unwrap()
}

fn render(t: &Templates, name: &str) -> String {
    let mut ctx = tera::Context::new();
    ctx.insert("x", "<b>");
    t.tera()
        .render(name, &ctx)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

#[test]
fn fallback_prefixes_user_then_themes_then_embedded() {
    let t = loaded(
        vec![
            user(
                "single.html",
                r#"{% include "_partials/a.html" %}|{% include "_partials/b.html" %}|{% include "_partials/c.html" %}"#,
            ),
            user("_partials/a.html", "user"),
            theme(1, "_partials/a.html", "theme1"),
            theme(1, "_partials/b.html", "theme1"),
            theme(2, "_partials/b.html", "theme2"),
            theme(2, "_partials/c.html", "theme2"),
            embedded("_partials/c.html", "embedded"),
        ],
        &Selections::new(),
    );
    assert_eq!(render(&t, "single.html"), "user|theme1|theme2");
    // Every file stays addressable by its prefixed name.
    assert_eq!(render(&t, "_embedded/_partials/c.html"), "embedded");
}

#[test]
fn escaping_follows_the_output_format() {
    let inc = r#"{{ x }}{% include "_partials/raw.txt" %}"#;
    let t = loaded(
        vec![
            user("single.html", inc),
            user("home.json", inc),
            user("robots.txt", "{{ x }}"),
            user("feed.plainxml.xml", "{{ x }}"),
            user("list.htmlish.txt", "{{ x }}"),
            user("list.rss.xml", "{{ x }}"),
            user("_partials/raw.txt", "[{{ x }}]"),
        ],
        &Selections::new(),
    );
    let s = t.store();
    let name = |n: &str| s.template(n).unwrap().render_name().to_string();
    assert_eq!(render(&t, &name("single.html")), "&lt;b&gt;[&lt;b&gt;]");
    assert_eq!(
        render(&t, &name("home.json")),
        "<b>[<b>]",
        "includes follow the caller"
    );
    assert_eq!(render(&t, &name("robots.txt")), "<b>");
    assert_eq!(name("feed.plainxml.xml"), "feed.plainxml.xml@@plain");
    assert_eq!(render(&t, &name("feed.plainxml.xml")), "<b>");
    assert_eq!(name("list.htmlish.txt"), "list.htmlish.txt@@escaped.html");
    assert_eq!(render(&t, &name("list.htmlish.txt")), "&lt;b&gt;");
    assert_eq!(render(&t, &name("list.rss.xml")), "&lt;b&gt;");
}

#[test]
fn base_variants_for_non_root_bases() {
    let sources = vec![
        user("baseof.html", "<root>{% block main %}{% endblock %}</root>"),
        user(
            "docs/baseof.html",
            "<docs>{% block main %}{% endblock %}</docs>",
        ),
        user(
            "list.html",
            "{# a comment #}\n{%- extends 'baseof.html' -%}{% block main %}list {{ x }}{% endblock %}",
        ),
        user(
            "single.html",
            "{% extends \"baseof.html\" %}{% block main %}single{% endblock %}",
        ),
        user("page.html", "no base"),
        user("docs/page.html", "{% extends \"_partials/other.html\" %}"),
        user("_partials/other.html", "other"),
    ];
    let s = store(sources.clone());
    let html = s.env().formats().by_name("html").unwrap();
    let q = |path: &str, kind| {
        let key = ContentKey::from_source(path);
        s.select(&LayoutQuery {
            path: &key,
            kind: Some(kind),
            layout: None,
            exact_layout: false,
            lang: None,
            format: html,
        })
        .unwrap()
    };
    let docs = q("docs", PageKind::Section);
    assert_eq!(docs.layout.as_str(), "list.html");
    assert_eq!(docs.base.as_ref().unwrap().as_str(), "docs/baseof.html");
    assert_eq!(docs.render_as.as_str(), "list.html@@docs/baseof.html");
    let blog = q("blog", PageKind::Section);
    assert_eq!(blog.base.as_ref().unwrap().as_str(), "baseof.html");
    assert_eq!(
        blog.render_as.as_str(),
        "list.html",
        "the root base needs no variant"
    );
    // `page.html` beats `single.html` and has no base; an explicit extends is left alone.
    let page = q("blog/p", PageKind::Page);
    assert_eq!(
        (page.layout.as_str(), page.base.as_ref()),
        ("page.html", None)
    );
    let docs_page = q("docs/p", PageKind::Page);
    assert_eq!(docs_page.layout.as_str(), "docs/page.html");
    assert_eq!(docs_page.base, None);
    let cands: Vec<&str> = s
        .base_candidates(&docs.layout)
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(cands, ["baseof.html", "docs/baseof.html"]);

    let sel: Selections = [docs.clone(), blog.clone(), page].into_iter().collect();
    let t = load(Arc::clone(&s), &sel, &|_| {}).unwrap();
    assert_eq!(
        render(&t, docs.render_as.as_str()),
        "<docs>list &lt;b&gt;</docs>"
    );
    assert_eq!(
        render(&t, blog.render_as.as_str()),
        "<root>list &lt;b&gt;</root>"
    );
    // Without the selection the variant is not registered.
    let t = load(s, &Selections::new(), &|_| {}).unwrap();
    assert!(!t.tera().contains_template(docs.render_as.as_str()));
}

#[test]
fn uses_variable_follows_includes_and_parents() {
    let t = loaded(
        vec![
            user(
                "baseof.html",
                "{{ site.title }}{% block main %}{% endblock %}",
            ),
            user(
                "list.html",
                "{% extends \"baseof.html\" %}{% block main %}{{ page.title }}{% endblock %}",
            ),
            user(
                "_shortcodes/box.html",
                "{% include \"_partials/in.html\" %}",
            ),
            user("_partials/in.html", "{{ inner }}"),
            user("_shortcodes/plain.html", "{{ shortcode.name }}"),
        ],
        &Selections::new(),
    );
    let s = Arc::clone(t.store());
    let n = |x: &str| s.template(x).unwrap().name.clone();
    assert!(t.uses_variable(&n("_shortcodes/box.html"), "inner"));
    assert!(!t.uses_variable(&n("_shortcodes/plain.html"), "inner"));
    assert!(t.uses_variable(&n("list.html"), "site"));
    assert!(t.uses_variable(&n("list.html"), "page"));
    assert!(!t.uses_variable(&n("list.html"), "inner"));
    // Lookups through the loaded instance.
    let html = s.env().formats().by_name("html").unwrap();
    let lang = ssg_base::Idx::from_index(0);
    let key = ContentKey::from_source("blog/p");
    assert_eq!(
        t.shortcode("Box", &key, html, lang).unwrap().as_str(),
        "_shortcodes/box.html"
    );
    assert_eq!(t.shortcode("nosuch", &key, html, lang), None);
    assert_eq!(t.partial("in").unwrap().as_str(), "_partials/in.html");
}

#[test]
fn tera_errors_are_load_errors() {
    let s = store(vec![user("single.html", "{% if %}")]);
    let e = load(s, &Selections::new(), &|_| {}).unwrap_err();
    assert!(matches!(e, TemplateError::Tera(_)), "{e}");
    let s = store(vec![user("single.html", "{{ x | no_such_filter }}")]);
    assert!(load(s, &Selections::new(), &|_| {}).is_err());
    let s = store(vec![user("single.html", "{{ x | no_such_filter }}")]);
    let registered = load(s, &Selections::new(), &|t| {
        t.register_filter(
            "no_such_filter",
            |v: &str, _: tera::Kwargs, _: &tera::State| v.to_owned(),
        );
    });
    assert!(
        registered.is_ok(),
        "the callback registers before the templates are added"
    );
}

fn read_tree(root: &Path, below: &str, out: &mut Vec<(String, String)>) {
    let Ok(rd) = fs::read_dir(root.join(below)) else {
        return;
    };
    let mut entries: Vec<_> = rd.map(|e| e.unwrap()).collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for e in entries {
        let name = e.file_name().into_string().unwrap();
        let rel = if below.is_empty() {
            name
        } else {
            format!("{below}/{name}")
        };
        if e.file_type().unwrap().is_dir() {
            read_tree(root, &rel, out);
        } else {
            out.push((rel, fs::read_to_string(e.path()).unwrap()));
        }
    }
}

/// Every converted site under `sites/` loads, with the contract's placeholders for the
/// template API and empty stubs for the embedded templates T32 has not written yet.
#[test]
fn converted_site_layouts_load() {
    let sites = ssg_testkit::fixture::repo_dir().join("sites");
    let mut dirs: Vec<_> = fs::read_dir(&sites)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("layouts").is_dir())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty());
    for dir in dirs {
        let mut files = Vec::new();
        read_tree(&dir.join("layouts"), "", &mut files);
        let mut sources: Vec<LayoutSource> = files
            .iter()
            .map(|(rel, src)| LayoutSource {
                rel: rel.clone(),
                origin: Origin::User(dir.join("layouts").join(rel)),
                source: src.clone(),
            })
            .collect();
        for name in ssg_funcs::spec::EMBEDDED_TEMPLATES {
            if !ssg_layouts::embedded::TEMPLATES
                .iter()
                .any(|(n, _)| n == name)
            {
                sources.push(embedded(name, ""));
            }
        }
        sources.extend(
            ssg_layouts::embedded::TEMPLATES
                .iter()
                .map(|(n, s)| embedded(n, s)),
        );
        let s = LayoutStore::from_sources(env(), sources)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        load(Arc::new(s), &Selections::new(), &|t| {
            ssg_funcs::register_placeholders(t);
        })
        .unwrap_or_else(|e| panic!("{}: {e:?}", dir.display()));
    }
}
