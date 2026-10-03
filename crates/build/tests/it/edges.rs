//! The edge-tree build oracles `testdata/oracle/sitebuild/build/<site>.json.gz` (T24's
//! sites: cascade, i18n, aliases, term collisions, headless and `build` options, multihost,
//! ugly URLs, Thai and punctuated paths, front matter overrides, stats, post-processing, …)
//! built into a temporary publish directory with their Go layouts converted to Tera below, and
//! compared with the files
//! the Go build wrote (L1: the same paths after fingerprint normalisation), and whether the
//! build reports errors.
//!
//! `docs` is the docs content tree (948 pages) with stub layouts and shortcodes.
//!
//! `content` and `shortcodes` use the shortcode and hook conversions of
//! `ssg-render`'s content oracles. Not here: `build-errors` (a failing build;
//! `ssg-cli`'s error report).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value as J;
use ssg_base::diag::Severity;
use ssg_build::{BuildError, BuildRequest, SinkKind, build};
use ssg_testkit::fixture::{oracle, repo_file};

use crate::support::{files_below, normalize_fingerprints, write_files};

const PAGER: &str = r#"{% set pager = paginator() %}<nav class="pager" id="pager-{{ pager.page_number }}"><span class="pn">{{ pager.page_number }}/{{ pager.total_pages }}</span>{% for p in pager.pages %}<a class="item" href="{{ p.rel_permalink }}">{{ p.title }}</a>{% endfor %}</nav>"#;

fn common() -> Vec<(&'static str, String)> {
    vec![
        (
            "layouts/404.html",
            format!(
                r#"<!doctype html><html><head><title>404</title></head><body class="nf">{PAGER}</body></html>"#
            ),
        ),
        (
            "layouts/home.html",
            format!(
                r#"<!doctype html><html><head><title>{{{{ page.title }}}}</title></head><body class="home"><h1 id="title">{{{{ page.title }}}}</h1>{PAGER}</body></html>"#
            ),
        ),
        (
            "layouts/list.html",
            format!(
                r#"<!doctype html><html><head><title>{{{{ page.title }}}}</title></head><body class="list {{{{ page.kind }}}}"><h1 id="title">{{{{ page.title }}}}</h1>{PAGER}</body></html>"#
            ),
        ),
        (
            "layouts/single.html",
            r#"<!doctype html><html><head><title>{{ page.title }}</title></head><body class="single"><article id="main">{{ page.title }}</article></body></html>"#.to_owned(),
        ),
    ]
}

/// The `docs`, `synthetic` and `testsite` fixture layouts.
fn synthetic() -> Vec<(&'static str, String)> {
    [
        (
            "layouts/404.html",
            "404|{% set pager = paginator() %}{% for p in pager.pages %}{{ p.rel_permalink }}{% endfor %}",
        ),
        (
            "layouts/list.html",
            "{{ page.title }}|{% if page.is_node %}{% set pager = paginator() %}{% for p in pager.pages %}{{ p.rel_permalink }}|{{ p.summary }}{% endfor %}{% else %}{{ page.content }}{% endif %}{% for f in page.alternative_output_formats %}{{ f.rel_permalink }}{% endfor %}",
        ),
        (
            "layouts/list.json",
            "{% for p in page.pages %}{{ p.rel_permalink }}{% endfor %}",
        ),
        (
            "layouts/single.html",
            "{{ page.title }}|{{ page.content }}|{{ page.summary }}|{% for f in page.output_formats %}{{ f.permalink }}{% endfor %}",
        ),
        ("layouts/single.json", "{{ page.rel_permalink }}"),
    ]
    .into_iter()
    .map(|(p, t)| (p, t.to_owned()))
    .collect()
}

/// The layouts of a fixture: the common set, with the fixture's own files over it.
fn layouts(name: &str) -> Vec<(&'static str, String)> {
    let own: Vec<(&'static str, &'static str)> = match name {
        "asm-multihost" | "asm-taxo" => vec![("layouts/_shortcodes/x.html", "x")],
        "contentdir" => vec![("layouts/_shortcodes/n.html", "{{ inner }}")],
        "docs" => vec![
            (
                "layouts/_markup/render-image.html",
                r#"<img src="{{ destination | safe }}" alt="{{ plain_text }}">"#,
            ),
            (
                "layouts/_markup/render-link.html",
                r#"<a href="{{ destination | safe }}">{{ text }}</a>"#,
            ),
        ],
        "edge-tree" => vec![(
            "layouts/_shortcodes/badge.html",
            r#"{{ shortcode | arg(index=0, default="") }}"#,
        )],
        "build-aliases" => vec![
            (
                "layouts/home.json",
                r#"{"title": {{ page.title | jsonify }}}"#,
            ),
            ("layouts/robots.txt", "User-agent: *"),
            (
                "layouts/single.html",
                r#"<!doctype html><html><head><title>{{ page.title }}</title></head><body class="single {{ page.section }}"><article id="main-{{ page.lang }}">{{ page.title }} <a href="/abs/link/">abs</a></article></body></html>"#,
            ),
            (
                "layouts/single.print.html",
                r#"<!doctype html><html><body class="print">{{ page.title }}</body></html>"#,
            ),
        ],
        "build-collide" => vec![
            ("layouts/robots.txt", "User-agent: *"),
            (
                "layouts/sitemap.xml",
                r#"<?xml version="1.0" encoding="utf-8" standalone="yes" ?><urlset>{% for p in page.pages %}<url><loc>{{ p.permalink }}</loc></url>{% endfor %}</urlset>"#,
            ),
        ],
        "build-custom-alias" => vec![(
            "layouts/alias.html",
            r#"<!DOCTYPE html><html><head><title>{{ permalink }}</title><link rel="canonical" href="{{ permalink }}"><meta http-equiv="refresh" content="0; url={{ permalink }}"></head><body>{% if page %}<p class="kind">{{ page.kind }} {{ page.title }}</p>{% else %}<p class="root">root</p>{% endif %}<p class="lang">{{ site.language.lang }}</p></body></html>"#,
        )],
        "build-stats-classes" | "build-stats-notags" => vec![
            (
                "layouts/home.html",
                r##"<!doctype html><html><head><title>{{ page.title }}</title></head><body class="home b-{{ page.kind }}"><div id="app" class="x  y z-{{ page.lang }}"><span class='single'>{{ page.title }}</span><svg class="icon"><use href="#i"></use></svg><i class="md:w-1/2 hover:bg-[#fff] a&b"></i></div><script>var s = "<div class='no'>";</script>{% set pager = paginator() %}<nav class="pager" id="pager-{{ pager.page_number }}"><span class="pn">{{ pager.page_number }}/{{ pager.total_pages }}</span>{% for p in pager.pages %}<a class="item" href="{{ p.rel_permalink }}">{{ p.title }}</a>{% endfor %}</nav></body></html>"##,
            ),
            (
                "layouts/single.html",
                r#"<!doctype html><html><body class="single"><p id="p-{{ page.file.base_file_name }}" class="{{ page.params.cls or "" }}">{{ page.title }}</p><!-- <div class="comment"> --></body></html>"#,
            ),
        ],
        "build-postprocess" => vec![
            (
                "layouts/home.html",
                r#"{% set css = get_asset(path="css/main.css") | minify | fingerprint | post_process %}<!doctype html><html><head><link rel="stylesheet" href="{{ css.rel_permalink }}" integrity="{{ css.data.integrity }}"><style>{{ css | resource_content | safe }}</style></head><body data-name="{{ css.name }}" data-mt="{{ css.media_type.type }}" data-pl="{{ css.permalink }}">{% set pager = paginator() %}<nav class="pager" id="pager-{{ pager.page_number }}"><span class="pn">{{ pager.page_number }}/{{ pager.total_pages }}</span>{% for p in pager.pages %}<a class="item" href="{{ p.rel_permalink }}">{{ p.title }}</a>{% endfor %}</nav></body></html>"#,
            ),
            (
                "layouts/home.json",
                r#"{% set css = get_asset(path="css/main.css") | minify | fingerprint | post_process %}{"css": {{ css.rel_permalink | jsonify }}, "again": {{ css.rel_permalink | jsonify }}}"#,
            ),
            (
                "layouts/list.html",
                r#"{% set o = get_asset(path="css/other.css") | post_process %}<!doctype html><html><head><link rel="stylesheet" href="{{ o.rel_permalink }}"></head><body class="list">{% set pager = paginator() %}<nav class="pager" id="pager-{{ pager.page_number }}"><span class="pn">{{ pager.page_number }}/{{ pager.total_pages }}</span>{% for p in pager.pages %}<a class="item" href="{{ p.rel_permalink }}">{{ p.title }}</a>{% endfor %}</nav></body></html>"#,
            ),
            (
                "layouts/single.html",
                r#"{% set css = get_asset(path="css/main.css") | minify | fingerprint | post_process %}<!doctype html><html><head><link rel="stylesheet" href="{{ css.rel_permalink }}"></head><body class="single">{{ page.title }}</body></html>"#,
            ),
        ],
        "content" => {
            let mut v = CONTENT_LAYOUTS.to_vec();
            v.extend([
                ("layouts/home.html", "{{ page.content }}"),
                ("layouts/page.html", "{{ page.content }}"),
                ("layouts/section.html", "{{ page.content }}"),
            ]);
            v
        }
        "shortcodes" => SHORTCODES_LAYOUTS.to_vec(),
        _ => Vec::new(),
    };
    let mut all: BTreeMap<&'static str, String> = match name {
        "docs" | "synthetic" | "testsite" => synthetic().into_iter().collect(),
        "content" => BTreeMap::new(),
        _ => common().into_iter().collect(),
    };
    for (p, t) in own {
        all.insert(p, t.to_owned());
    }
    all.into_iter().collect()
}

// The shortcode and hook conversions of `ssg-render`'s content oracles (`tests/it/oracle.rs`
// there), for the build fixtures of the same sites.

/// `{"head":…,"body":…}` of a table, cells as `"<alignment>:<text>"` (Go's `printf "%s:%s"`,
/// where alignment none prints nothing).
const JSON_TABLE: &str = r#"{"head":[{% for r in thead %}{% if not loop.first %},{% endif %}[{% for c in r %}{% if not loop.first %},{% endif %}{% if c.alignment == "none" %}{{ (":" ~ c.text) | jsonify }}{% else %}{{ (c.alignment ~ ":" ~ c.text) | jsonify }}{% endif %}{% endfor %}]{% endfor %}],"body":[{% for r in tbody %}{% if not loop.first %},{% endif %}[{% for c in r %}{% if not loop.first %},{% endif %}{% if c.alignment == "none" %}{{ (":" ~ c.text) | jsonify }}{% else %}{{ (c.alignment ~ ":" ~ c.text) | jsonify }}{% endif %}{% endfor %}]{% endfor %}]}"#;

const CONTENT_LAYOUTS: &[(&str, &str)] = &[
    (
        "layouts/_markup/render-blockquote.html",
        r#"<blockquote class="{{ type }}">{{ text }}</blockquote>"#,
    ),
    (
        "layouts/_markup/render-codeblock.html",
        r#"<pre class="cb" data-lang="{{ type }}"><code>{{ inner }}</code></pre>"#,
    ),
    (
        "layouts/_markup/render-heading.html",
        r#"<h{{ level }} id="{{ anchor }}">{{ text }} #{{ level }}</h{{ level }}>"#,
    ),
    (
        "layouts/_markup/render-heading.rss.xml",
        r"<h{{ level }}>{{ text }}</h{{ level }}>",
    ),
    (
        "layouts/_markup/render-image.html",
        r#"<img src="{{ destination | safe }}" alt="{{ plain_text }}">"#,
    ),
    (
        "layouts/_markup/render-link.html",
        r#"<a href="{{ destination | safe }}"{% if title %} title="{{ title }}"{% endif %}>{{ text }}</a>"#,
    ),
    ("layouts/_markup/render-table.json.json", JSON_TABLE),
    (
        "layouts/_shortcodes/deindent.html",
        "<pre>{{ inner_deindent }}</pre>\n<p>second\nline</p>",
    ),
    ("layouts/_shortcodes/fmt.html", "<b>html fmt</b>"),
    ("layouts/_shortcodes/fmt.rss.xml", "rss fmt"),
    ("layouts/_shortcodes/hl.html", "<mark>{{ inner }}</mark>"),
    (
        "layouts/_shortcodes/inner.html",
        r#"<span data-parent="{% if shortcode.parent %}{{ shortcode.parent.name }}{{ shortcode.parent.ordinal }}{% endif %}" data-ord="{{ shortcode.ordinal }}">{{ inner }}</span>"#,
    ),
    ("layouts/_shortcodes/md.html", "{{ inner }}"),
    (
        "layouts/_shortcodes/named.html",
        r#"[{{ shortcode | arg(name="a", default="") }}-{{ shortcode | arg(name="b", default="") }}-{{ shortcode | arg(index=0, default="") }}-{{ shortcode.is_named_params }}]"#,
    ),
    (
        "layouts/_shortcodes/outer.html",
        r#"<div class="outer" data-ord="{{ shortcode.ordinal }}">{{ inner }}</div>"#,
    ),
    (
        "layouts/_shortcodes/pos.html",
        r#"[{{ shortcode | arg(index=0, default="") }}|{{ shortcode | arg(index=1, default="") }}|{{ shortcode | arg(index=9, default="") }}|{{ shortcode.is_named_params }}|{{ shortcode.ordinal }}]"#,
    ),
    ("layouts/_shortcodes/title.html", "{{ page.title }}"),
    (
        "layouts/_shortcodes/toc.html",
        r#"<nav class="toc">{{ page_toc(page=page) }}</nav>"#,
    ),
    ("layouts/_shortcodes/v1.html", "<em>{{ inner }}</em>"),
];

const SHORTCODES_LAYOUTS: &[(&str, &str)] = &[
    (
        "layouts/_shortcodes/inner.html",
        "{% if inner %}{{ inner }}{% endif %}",
    ),
    ("layouts/_shortcodes/inner2.html", "{{ inner }}"),
    ("layouts/_shortcodes/inner3.html", "{{ inner }}"),
    ("layouts/_shortcodes/legacytag.html", "tag"),
    ("layouts/_shortcodes/sc1.html", "sc1"),
    ("layouts/_shortcodes/sc2.html", "sc2"),
    ("layouts/_shortcodes/tag.html", "tag"),
];

/// Content rewrites: inline shortcode bodies are Tera templates.
const REWRITES: &[(&str, &str, &str)] = &[
    ("edge-tree", "{{ .Page.Title }}", "{{ page.title }}"),
    ("content", "{{ .Page.Title }}", "{{ page.title }}"),
    (
        "contentdir",
        "{{ .Get 0 }}",
        r#"{{ shortcode | arg(index=0, default="") }}"#,
    ),
];

/// Accepted differences of the file lists (left out on both sides): (site, path, reason).
const EXPECTED: &[(&str, &str, &str)] = &[
    (
        "build-postprocess",
        "js/main.js",
        "the Go layout bundles `js/main.js` with `js.Build`; the conversion, made when the \
         offline tests had no bundler, leaves the call out",
    ),
    (
        "build-postprocess",
        "css/main.css",
        "the home page prints the asset's `.Name`, `/css/main.css`, which is also its URL: \
         URL-token publishing (REWRITE_PLAN.md §3.4) publishes it; Go publishes a resource \
         only when its links are asked for",
    ),
];

/// The sites compared.
const SITES: &[&str] = &[
    "asm-build",
    "asm-cascade",
    "asm-flags",
    "asm-i18n",
    "asm-multihost",
    "asm-taxo",
    "asm-ugly",
    "build-aliases",
    "build-collide",
    "build-custom-alias",
    "build-disable",
    "build-multihost",
    "build-postprocess",
    "build-stats-classes",
    "build-stats-notags",
    "content",
    "contentdir",
    "docs",
    "edge-tree",
    "homeleaf",
    "nokinds",
    "shortcodes",
    "synthetic",
    "testsite",
];

struct Outcome {
    want: usize,
    equal: usize,
    collisions: usize,
    unexpected: Vec<String>,
}

/// Writes the site of fixture `name` (its files, the Tera layouts) into `dir`; returns the
/// fixture.
/// FNV-1a 64 of `b`, as the oracle records repository files.
fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &c in b {
        h ^= u64::from(c);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

pub(crate) fn write_site(name: &str, dir: &std::path::Path) -> J {
    let f: J = oracle(&format!("oracle/sitebuild/build/{name}.json.gz"));
    let site = &f["site"];
    let mut toml = site["toml"].as_str().expect("toml").to_owned();
    if name == "docs" {
        // Plain fences, as under the I01 docs patches: the embedded goat hook needs the
        // `diagrams_goat` feature (A-D2), which the tests do not compile. Contents only.
        toml = toml.replacen(
            "  [markup.highlight]\n",
            "  [markup.highlight]\n    codeFences = false\n",
            1,
        );
    }
    let mut files = vec![("config.toml".to_owned(), toml)];
    for file in site["files"].as_array().expect("files") {
        let path = file["path"].as_str().expect("path");
        if path.starts_with("layouts/") {
            // The docs fixture's shortcodes are stubs: empty, or `{{ with .Inner }}{{ . }}{{ end }}`.
            if name == "docs" && path.starts_with("layouts/_shortcodes/") {
                let stub = if file["content"].as_str().unwrap_or_default().is_empty() {
                    ""
                } else {
                    "{{ inner }}"
                };
                files.push((path.to_owned(), stub.to_owned()));
            }
            continue;
        }
        // Repository files (the docs content) are recorded by path and hash.
        if let Some(r) = file["repo"].as_str() {
            let bytes = std::fs::read(repo_file(r)).unwrap_or_else(|e| panic!("{r}: {e}"));
            assert_eq!(
                fnv(&bytes),
                file["fnv"].as_str().expect("fnv"),
                "{r} changed since the oracle run"
            );
            let to = dir.join(path);
            std::fs::create_dir_all(to.parent().expect("parent")).expect("mkdir");
            std::fs::write(to, bytes).expect("write");
            continue;
        }
        let mut content = file["content"].as_str().expect("content").to_owned();
        for (s, from, to) in REWRITES {
            if *s == name {
                content = content.replace(from, to);
            }
        }
        files.push((path.to_owned(), content));
    }
    files.extend(layouts(name).into_iter().map(|(p, t)| (p.to_owned(), t)));
    write_files(dir, &files);
    f
}

fn check(name: &str) -> Outcome {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("site");
    let public = tmp.path().join("public");
    let f = write_site(name, &dir);
    // A disk build: like Go's, it writes its files even when it reports errors.
    let report = build(BuildRequest {
        source: dir,
        destination: Some(public.clone()),
        sink: SinkKind::Disk,
        // The clock of the oracle runs (docs pages expiring by then are gone).
        clock: Some("2026-09-01T00:00:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    });
    let go_errors = f["log"]
        .as_array()
        .expect("log")
        .iter()
        .filter(|l| l.as_str().is_some_and(|l| l.starts_with("ERROR")))
        .count();
    let mut unexpected = Vec::new();
    let collisions = report.as_ref().map_or(0, |r| r.collisions.len());
    let errors: Vec<String> = match &report {
        Ok(r) => {
            if !r.collisions.is_empty() {
                println!("{name}: {} collisions", r.collisions.len());
            }
            if std::env::var_os("FUGO_T36_TIMINGS").is_some() {
                println!("{name}: {:?}", r.timings);
            }
            Vec::new()
        }
        Err(BuildError::Diagnostics(ds)) => ds
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.message.clone())
            .collect(),
        Err(e) => {
            return Outcome {
                want: 0,
                equal: 0,
                collisions: 0,
                unexpected: vec![format!("{name}: build failed: {e}")],
            };
        }
    };
    // Go names the file in each message; ours are de-duplicated by message and position, so
    // only whether the build reports errors is compared.
    if errors.is_empty() != (go_errors == 0) {
        unexpected.push(format!(
            "{name}: {} errors, Go {go_errors}: {errors:?}",
            errors.len()
        ));
    }
    let accepted: BTreeSet<&str> = EXPECTED
        .iter()
        .filter(|(s, _, _)| *s == name)
        .map(|(_, p, _)| *p)
        .collect();
    let want: BTreeSet<String> = f["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter_map(|e| e["path"].as_str())
        .filter(|p| !accepted.contains(p))
        .map(normalize_fingerprints)
        .collect();
    let got: BTreeSet<String> = files_below(&public)
        .iter()
        .filter(|p| !accepted.contains(p.as_str()))
        .map(|p| normalize_fingerprints(p))
        .collect();
    for p in want.difference(&got) {
        unexpected.push(format!("{name}: missing {p}"));
    }
    for p in got.difference(&want) {
        unexpected.push(format!("{name}: extra {p}"));
    }
    Outcome {
        want: want.len(),
        equal: want.intersection(&got).count(),
        collisions,
        unexpected,
    }
}

#[test]
fn edge_trees_match_the_go_file_lists() {
    let mut all = Vec::new();
    let (mut want, mut equal) = (0, 0);
    for name in SITES {
        let o = check(name);
        println!(
            "{name}: {}/{} files equal, {} unexpected",
            o.equal,
            o.want,
            o.unexpected.len()
        );
        want += o.want;
        equal += o.equal;
        if *name == "build-collide" && o.collisions == 0 {
            all.push("build-collide: no collision reported".to_owned());
        }
        all.extend(o.unexpected);
    }
    println!(
        "edge trees: {equal}/{want} files equal over {} sites",
        SITES.len()
    );
    for u in &all {
        println!("{u}");
    }
    assert!(all.is_empty(), "{} unexpected differences", all.len());
}
