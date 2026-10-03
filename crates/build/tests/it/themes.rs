//! A project built with a theme (`ssg-config`'s theme merge, `ssg-vfs`'s theme
//! mounts): the theme's params, menus and output format take effect below the project's
//! configuration, from `config.toml` (a `config.yaml` next to it is ignored with a warning).

use ssg_build::{BuildRequest, SinkKind, build};

use crate::support::write_files;

const FILES: &[(&str, &str)] = &[
    (
        "config.toml",
        r#"baseURL = "https://example.org/"
title = "Project"
theme = "demo"
disableKinds = ["taxonomy", "term", "rss", "sitemap", "robotstxt", "404"]
[params]
greeting = "Hello from the project"
[outputs]
home = ["html", "searchindex"]
"#,
    ),
    ("config.yaml", "title: Ignored\n"),
    (
        "themes/demo/config.toml",
        r#"title = "Theme title (a root value: not merged)"
[params]
greeting = "Hello from the theme"
footer = "Theme footer"
[params.social]
x = "@demo"
[[menus.main]]
name = "Home"
pageRef = "/"
weight = 1
[[menus.main]]
name = "About"
url = "/about/"
weight = 2
[outputFormats.searchindex]
mediaType = "application/json"
baseName = "searchindex"
isPlainText = true
"#,
    ),
    (
        "themes/demo/layouts/home.html",
        r#"<h1>{{ site.title }}</h1><p class="greeting">{{ site.params.greeting }}</p><nav>{% for e in site.menus.main %}<a href="{{ e.url }}">{{ e.name }}</a>{% endfor %}</nav><footer>{{ site.params.footer }} {{ site.params.social.x }}</footer>"#,
    ),
    (
        "themes/demo/layouts/home.searchindex.json",
        r#"{"title": {{ site.title | jsonify }}, "pages": [{% for p in site.regular_pages %}{% if not loop.first %}, {% endif %}{{ p.title | jsonify }}{% endfor %}]}"#,
    ),
    (
        "themes/demo/layouts/single.html",
        "<h1>{{ page.title }}</h1>{{ page.content }}",
    ),
    ("themes/demo/layouts/list.html", "<h1>{{ page.title }}</h1>"),
    ("content/_index.md", "---\ntitle: Home\n---\n"),
    ("content/about.md", "---\ntitle: About\n---\nAbout us.\n"),
];

#[test]
fn theme_params_menus_and_output_format() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("site");
    let files: Vec<(String, String)> = FILES
        .iter()
        .map(|(p, c)| ((*p).to_owned(), (*c).to_owned()))
        .collect();
    write_files(&site, &files);
    let report = build(BuildRequest {
        source: site.clone(),
        sink: SinkKind::Memory,
        clock: Some("2026-01-01T00:00:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    })
    .unwrap_or_else(|e| panic!("build: {e}"));
    let mem = report.memory.as_ref().expect("memory sink");
    let text = |p: &str| {
        mem.text(p)
            .unwrap_or_else(|| panic!("no {p}: {:?}", mem.paths()))
    };

    // The theme's layouts, the project's title and greeting, the theme's other params and
    // its menu.
    assert_eq!(
        text("index.html"),
        r#"<h1>Project</h1><p class="greeting">Hello from the project</p><nav><a href="/">Home</a><a href="/about/">About</a></nav><footer>Theme footer @demo</footer>"#
    );
    // The output format the theme defines, used by the project's `outputs`.
    assert_eq!(
        text("searchindex.json"),
        r#"{"title": "Project", "pages": ["About"]}"#
    );
    assert_eq!(text("about/index.html"), "<h1>About</h1><p>About us.</p>\n");

    // config.toml is read; the config.yaml next to it is reported.
    let warnings: Vec<String> = report
        .diagnostics
        .iter()
        .filter(|d| d.id.as_deref() == Some("config-file-ignored"))
        .map(ToString::to_string)
        .collect();
    assert_eq!(warnings.len(), 1, "{:?}", report.diagnostics);
    assert!(
        warnings[0].contains("using config.toml; ignoring config.yaml"),
        "{}",
        warnings[0]
    );
}
