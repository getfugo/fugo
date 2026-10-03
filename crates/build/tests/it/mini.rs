//! The e2e `mini` site (`testdata/oracle/commands/e2e/mini.txtar`: en/th, the real
//! function set, render hooks, shortcodes, pagination, taxonomies, menus, i18n, data, related
//! content, aliases, `templates.Defer`, `GetRemote` from the file cache + `unmarshal`, and
//! resource pipelines: minify, fingerprint, Concat, ExecuteAsTemplate, FromString,
//! PostProcess) with its Go layouts converted to Tera below, built into a `MemorySink` with the
//! oracle's flags (`--minify --clock 2026-09-27T12:00:00Z`; the cache directory `site/_cache`,
//! which the oracle set through the Go program's environment) and compared with the Go build's
//! tree (`e2e.json.gz`, case `mini`).

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value as J;
use ssg_build::{BuildReport, BuildRequest, SinkKind, build};
use ssg_config::CliOverrides;
use ssg_testkit::fixture::oracle;
use ssg_testkit::txtar::Archive;

use crate::support::{normalize_fingerprints, write_files};

const BASEOF: &str = r#"<!DOCTYPE html>
<html lang="{{ page.lang }}">
<head>
<meta charset="utf-8">
<title>{% block title %}{{ page.title }} | {{ site.title }}{% endblock title %}</title>
{%- set css = get_asset(path="css/site.css") | minify | fingerprint %}{% if page.is_home %}{% set css = css | post_process %}{% endif %}
<link rel="stylesheet" href="{{ css.rel_permalink }}" integrity="{{ css.data.integrity }}">
{%- for t in page.translations %}<link rel="alternate" hreflang="{{ t.lang }}" href="{{ t.permalink }}">{% endfor %}
</head>
<body class="{{ page.kind }}">
<nav class="menu">{% for e in site.menus.main or [] %}<a class="{% if is_menu_current(menu="main", entry=e) %}on{% endif %}" href="{{ e.url | rel_lang_url }}">{{ e.name }}</a>{% endfor %}</nav>
{% block main %}{% endblock main %}
{{ defer(template="_partials/deferred/footer.html", key="footer") }}
{%- set a = get_asset(path="js/a.js") %}{% set b = get_asset(path="js/b.js") | execute_as_template(target="js/b.js", data={"api": site.params.api, "lang": page.lang}) %}
{%- set js = concat_assets(target="js/all.js", items=[a, b]) | minify | fingerprint %}
<script src="{{ js.rel_permalink }}"></script>
</body>
</html>
"#;

const SINGLE: &str = r#"{% extends "baseof.html" %}
{% block main %}
<article class="post">
<h1>{{ page.title }}</h1>
<p class="meta">{{ page.date | date(format="%Y-%m-%d") }} · {{ page.word_count }} {{ i18n(key="words", count=page.word_count) }} · {{ page.reading_time }}</p>
{{ page.content }}
{% if page.table_of_contents %}<aside class="toc">{{ page.table_of_contents }}</aside>{% endif %}
{% set terms = get_terms(taxonomy="tags") %}{% if terms %}<ul class="tags">{% for t in terms %}<li><a href="{{ t.rel_permalink }}">{{ t.link_title }}</a></li>{% endfor %}</ul>{% endif %}
{% set rel = related(pages=site.regular_pages)[:3] %}{% if rel %}<ul class="related">{% for p in rel %}<li>{{ p.title }}</li>{% endfor %}</ul>{% endif %}
{% if page.params.video %}{% include "_partials/video.html" %}{% endif %}
{% if page.prev_in_section %}<a class="prev" href="{{ page.prev_in_section.rel_permalink }}">{{ page.prev_in_section.title }}</a>{% endif %}
</article>
{% endblock main %}
"#;

const LIST: &str = r#"{% extends "baseof.html" %}
{% block main %}
<h1>{{ page.title }}</h1>
{{ page.content }}
{% set pager = paginator() %}{% for p in pager.pages %}<div class="item"><a href="{{ p.rel_permalink }}">{{ p.title }}</a> {{ p.summary }}</div>{% endfor %}
{% include "_partials/pager.html" %}
{% endblock main %}
"#;

const HOME: &str = r#"{% extends "baseof.html" %}
{% block title %}{{ site.title }}{% endblock title %}
{% block main %}
<h1>{{ i18n(key="welcome", data={"Name": site.title}) }}</h1>
{% set pager = paginator() %}{% for p in pager.pages %}<div class="item">{{ p.title }}</div>{% endfor %}
{% include "_partials/pager.html" %}
<ul class="tags">{% for t in site.taxonomies.tags | by_count %}<li>{{ t.page.title }} {{ t.count }}</li>{% endfor %}</ul>
<p class="data">{% for k, v in site.data.authors | sort_keys %}{{ k }}={{ v.name }};{% endfor %}</p>
<p class="n">{{ site.regular_pages | length }}</p>
{% set r = asset_from_string(target="gen/info.txt", content=site.title ~ " " ~ (site.pages | length)) %}<a href="{{ r.rel_permalink }}">info</a>
{% endblock main %}
"#;

const HOME_JSON: &str = r#"{{- {"lang": page.lang, "items": [{"title": p.title, "url": p.permalink, "plain": p.plain | truncate(length=40)} for p in site.regular_pages]} | jsonify -}}"#;

/// Go renders term pages with the legacy `_default/taxonomy.html` (v0.146 maps the old
/// `taxonomy` name to terms; the site's `term.html` is not used), whose term list is empty on a
/// term page: no paginator, so no `page/1` alias.
const TERM: &str = r#"{% extends "baseof.html" %}
{% block main %}<h1>{{ page.title }}</h1>{% endblock main %}
"#;

const TAXONOMY: &str = r#"{% extends "baseof.html" %}
{% block main %}<h1>{{ page.title }}</h1>{% for t in page.taxonomy.terms | alphabetical %}<a href="{{ t.page.rel_permalink }}">{{ t.page.title }} {{ t.count }}</a>{% endfor %}{% endblock main %}
"#;

const NOT_FOUND: &str = r#"{% extends "baseof.html" %}
{% block main %}<p class="nf">{{ i18n(key="notFound") }}</p>{% endblock main %}
"#;

const PAGER: &str = r#"{%- set pager = paginator() %}{% if pager.total_pages > 1 %}<nav class="pager">{% for p in pager.pagers %}<a class="{% if p.page_number == pager.page_number %}cur{% endif %}" href="{{ p.url }}">{{ p.page_number }}</a>{% endfor %}</nav>{% endif -%}"#;

const VIDEO: &str = r#"{%- set url = "https://www.googleapis.com/youtube/v3/videos?key=API_KEY&part=snippet,contentDetails,statistics&id=" ~ page.params.video %}
{%- set r = get_remote(url=url, optional=true) %}{% if r %}{% set d = r | unmarshal %}{% for v in d["items"] %}<div class="video" data-id="{{ v.id }}">{{ v.snippet.title }} ({{ v.statistics.viewCount }} views, {{ v.contentDetails.duration }})</div>{% endfor %}{% endif -%}"#;

const FOOTER: &str = r#"<footer class="deferred">{{ site.regular_pages | length }} pages, {{ build.environment }}</footer>"#;

const HEADING: &str =
    r##"<h{{ level }} id="{{ anchor }}">{{ text }}<a href="#{{ anchor }}">#</a></h{{ level }}>"##;

const LINK: &str = r#"<a href="{{ destination | safe }}"{% if destination is starting_with(pat="http") %} rel="external"{% endif %}>{{ text }}</a>"#;

const NOTE: &str = r#"<div class="note">{{ inner | markdownify }}</div>"#;

const PARAM: &str = r#"{% set k = shortcode | arg(index=0, default="") %}<code>{{ k }}={{ page.params[k] }}</code>"#;

/// `execute_as_template` assets are Tera templates.
const B_JS: &str = "var api = \"{{ data.api }}\";\nvar lang = \"{{ data.lang }}\";\nconsole.log(greet(lang), api);\n";

const LAYOUTS: &[(&str, &str)] = &[
    ("layouts/baseof.html", BASEOF),
    ("layouts/single.html", SINGLE),
    ("layouts/list.html", LIST),
    ("layouts/home.html", HOME),
    ("layouts/home.json", HOME_JSON),
    ("layouts/term.html", TERM),
    ("layouts/taxonomy.html", TAXONOMY),
    ("layouts/404.html", NOT_FOUND),
    ("layouts/_partials/pager.html", PAGER),
    ("layouts/_partials/video.html", VIDEO),
    ("layouts/_partials/deferred/footer.html", FOOTER),
    ("layouts/_markup/render-heading.html", HEADING),
    ("layouts/_markup/render-link.html", LINK),
    ("layouts/_shortcodes/note.html", NOTE),
    ("layouts/_shortcodes/param.html", PARAM),
    ("assets/js/b.js", B_JS),
];

/// The `mini` case of the e2e oracle.
fn case() -> J {
    let e2e: J = oracle("oracle/commands/e2e/e2e.json.gz");
    e2e["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["name"] == "mini")
        .expect("mini case")
        .clone()
}

/// Builds the site into memory (`threads`: the render pool size).
pub(crate) fn build_mini(threads: Option<usize>) -> (tempfile::TempDir, BuildReport) {
    let case = case();
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("site");
    let archive = Archive::read(&ssg_testkit::fixture::testdata(
        "oracle/commands/e2e/mini.txtar",
    ))
    .expect("mini.txtar");
    let mut files: Vec<(String, String)> = archive
        .files
        .iter()
        .filter(|f| !f.name.starts_with("layouts/"))
        .map(|f| (f.name.clone(), f.data.clone()))
        .collect();
    // The getresource cache entry of the Go run (not in the txtar).
    for (name, content) in case["files"].as_object().expect("files") {
        if name.starts_with("_cache/") {
            files.push((name.clone(), content.as_str().expect("text").to_owned()));
        }
    }
    files.extend(
        LAYOUTS
            .iter()
            .map(|(p, c)| ((*p).to_owned(), (*c).to_owned())),
    );
    write_files(&site, &files);
    let report = build(BuildRequest {
        source: site.clone(),
        sink: SinkKind::Memory,
        clock: Some("2026-09-27T12:00:00Z".parse().expect("clock")),
        cli: CliOverrides {
            minify: Some(true),
            cache_dir: Some(site.join("_cache")),
            ..CliOverrides::default()
        },
        threads,
        ..BuildRequest::default()
    })
    .unwrap_or_else(|e| panic!("mini build: {e}"));
    (tmp, report)
}

#[test]
fn mini_matches_the_go_tree() {
    let case = case();
    let (_tmp, report) = build_mini(None);
    let mem = report.memory.as_ref().expect("memory sink");
    let want: BTreeSet<String> = case["result"]["tree"]
        .as_object()
        .expect("tree")
        .keys()
        .filter_map(|k| k.strip_prefix("out/"))
        .map(normalize_fingerprints)
        .collect();
    let got: BTreeSet<String> = mem
        .paths()
        .iter()
        .map(|p| normalize_fingerprints(p.relative()))
        .collect();
    let missing: Vec<&String> = want.difference(&got).collect();
    let extra: Vec<&String> = got.difference(&want).collect();
    println!(
        "mini: {} files (Go {}), {} equal; missing {missing:?}, extra {extra:?}; {:?}",
        got.len(),
        want.len(),
        got.intersection(&want).count(),
        (
            report.outputs,
            report.aliases,
            report.resources,
            report.collisions.len()
        )
    );
    for d in &report.diagnostics {
        println!("diagnostic: {d:?}");
    }
    if let Some(dir) = std::env::var_os("FUGO_T36_OUT") {
        mem.write_to(Path::new(&dir)).expect("write");
    }
    assert_eq!(got, want, "file list differs from the Go build's");

    // No placeholder survives; the deferred footer is in every HTML page.
    for p in mem.paths() {
        let bytes = mem.get(p.relative()).expect("file");
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("__nh_defer_") && !text.contains("__nh_pp_"),
            "{p}: {text}"
        );
    }
    let text = |p: &str| mem.text(p).unwrap_or_else(|| panic!("no {p}"));
    let one = text("posts/one/index.html");
    assert!(
        one.contains(r#"<footer class=deferred>4 pages, production</footer>"#),
        "{one}"
    );
    // GetRemote from the file cache + unmarshal.
    assert!(one.contains("data-id=ZaBhxhXAU7w"), "{one}");
    // The PostProcess'd CSS of the home page: its link and integrity resolved in E5, the file
    // published in E6 from the patched output's URL tokens.
    let home = text("index.html");
    let css = mem
        .paths()
        .into_iter()
        .find(|p| p.relative().starts_with("css/site.min."))
        .expect("css published");
    assert!(home.contains(css.relative()), "{home}");
    assert!(home.contains("integrity=\"sha256-"), "{home}");
    // ExecuteAsTemplate inside Concat.
    let js = mem
        .paths()
        .into_iter()
        .find(|p| p.relative().starts_with("js/all.min."))
        .expect("js published");
    assert!(text(js.relative()).contains("https://api.example.org"));
    // Go writes the Thai site's `FromString` last (a later language overwrites the file); here
    // the earlier language's resource is the one published (REWRITE_PLAN.md §3.5).
    assert!(text("gen/info.txt").starts_with("Mini "));
    // The site's `[build.buildStats]` is reported, and no stats file is written.
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.id.as_deref() == Some("deprecated-config-build.buildstats")),
        "{:?}",
        report.diagnostics
    );
}
