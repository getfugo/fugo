//! Content adapters (`content/**/_content.html`): Go's integration tests of
//! `pagesfromdata/pagesfromgotmpl_integration_test.go` with the adapters and layouts
//! converted to Tera, built in memory.

use std::fs;

use ssg_build::{BuildError, BuildReport, BuildRequest, SinkKind, build};
use ssg_config::CliOverrides;

use crate::support::write_files;

mod go_binary;
mod languages;
mod paths;

/// `assets/a/pixel.png` of Go's test: a 1×1 PNG.
const PIXEL: [u8; 70] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64, 0x60, 0xf8, 0x5f,
    0x0f, 0x00, 0x02, 0x87, 0x01, 0x80, 0xeb, 0x47, 0xba, 0x92, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// Builds a site of `files` (path, content) in memory.
fn try_build(files: &[(&str, &str)]) -> (tempfile::TempDir, Result<BuildReport, BuildError>) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("site");
    let files: Vec<(String, String)> = files
        .iter()
        .map(|(p, c)| ((*p).to_owned(), (*c).to_owned()))
        .collect();
    write_files(&site, &files);
    if files.iter().any(|(p, _)| p == "assets/a/pixel.png") {
        fs::write(site.join("assets/a/pixel.png"), PIXEL).expect("pixel");
    }
    let report = build(BuildRequest {
        source: site,
        sink: SinkKind::Memory,
        cli: CliOverrides {
            cache_dir: Some(tmp.path().join("cache")),
            ..CliOverrides::default()
        },
        ..BuildRequest::default()
    });
    (tmp, report)
}

fn build_ok(files: &[(&str, &str)]) -> (tempfile::TempDir, BuildReport) {
    let (tmp, r) = try_build(files);
    let r = r.unwrap_or_else(|e| match e {
        BuildError::Diagnostics(d) => panic!(
            "build failed: {}",
            d.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        ),
        e => panic!("build failed: {e}"),
    });
    (tmp, r)
}

/// The text of output `path`.
fn text(r: &BuildReport, path: &str) -> String {
    let mem = r.memory.as_ref().expect("memory");
    mem.text(path).unwrap_or_else(|| {
        let paths: Vec<String> = mem
            .paths()
            .iter()
            .map(|p| p.relative().to_owned())
            .collect();
        panic!("no output {path}: {paths:?}")
    })
}

fn exists(r: &BuildReport, path: &str) -> bool {
    r.memory.as_ref().expect("memory").text(path).is_some()
        || r.memory.as_ref().expect("memory").get(path).is_some()
}

fn assert_contains(haystack: &str, needles: &[&str]) {
    for n in needles {
        assert!(haystack.contains(n), "{n:?} not in:\n{haystack}");
    }
}

const BASIC_SINGLE: &str = r#"Single: {{ page.title }}|{{ page.content }}|Params: {{ page.params.param1 or "" }}|Path: {{ page.path }}|
Dates: Date: {{ page.date | date(format="%Y-%m-%d") }}|Lastmod: {{ page.lastmod | date(format="%Y-%m-%d") }}|PublishDate: {{ page.publish_date | date(format="%Y-%m-%d") }}|ExpiryDate: {{ page.expiry_date | date(format="%Y-%m-%d") }}|
Len Resources: {{ page.resources | length }}
Resources: {% for r in page.resources %}RelPermalink: {{ r.rel_permalink }}|Name: {{ r.name }}|Title: {{ r.title }}|Params: {{ r.params | jsonify }}|{% endfor %}$
{%- set featured = page.resources | get_resource(name="featured.png") %}
{%- if featured %}
Featured Image: {{ featured.rel_permalink }}|{{ featured.name }}|
{%- set small = featured | resize(spec="10x10") %}
Resized Featured Image: {{ small.width }}|
{%- endif %}
"#;

const BASIC_LIST: &str = r#"List: {{ page.title }}|
RegularPagesRecursive: {% for p in page.regular_pages_recursive %}{{ p.title }}:{{ p.path }}|{% endfor %}$
Sections: {% for s in page.sections %}{{ s.title }}:{{ s.path }}|{% endfor %}$
"#;

const BASIC_ADAPTER: &str = r#"{%- set pixel = get_asset(path="a/pixel.png") %}
{%- set data_resource = get_asset(path="mydata.yaml") %}
{%- set data = data_resource | unmarshal %}
{%- set pd = data.p1 %}
{%- set pp = partial(name="get-value.html") %}
{%- set title = pd ~ ":" ~ pp %}
{%- set dates = {"date": "2023-03-01" | to_date} %}
{%- set content_markdown = {"value": "**Hello World**", "mediaType": "text/markdown"} %}
{%- set content_markdown_default = {"value": "**Hello World Default**"} %}
{%- set content_html = {"value": "<b>Hello World!</b> No **markdown** here.", "mediaType": "text/html"} %}
{{- add_page(page={"kind": "page", "path": "P1", "title": title, "dates": dates, "keywords": ["foo", "Bar"], "content": content_markdown, "params": {"param1": "param1v"} }) }}
{{- add_page(page={"kind": "page", "path": "p2", "title": "p2title", "dates": dates, "content": content_html}) }}
{{- add_page(page={"kind": "page", "path": "p3", "title": "p3title", "dates": dates, "content": content_markdown_default, "draft": false}) }}
{{- add_page(page={"kind": "page", "path": "p4", "title": "p4title", "dates": dates, "content": content_markdown_default, "draft": data.draft}) }}
{%- set resource_content = {"value": data_resource} %}
{{- add_resource(resource={"path": "p1/data1.yaml", "content": resource_content}) }}
{{- add_resource(resource={"path": "p1/mytext.txt", "content": {"value": "some text"}, "name": "textresource", "title": "My Text Resource", "params": {"param1": "param1v"} }) }}
{{- add_resource(resource={"path": "p1/sub/mytex2.txt", "content": {"value": "some text"}, "title": "My Text Sub Resource"}) }}
{{- add_resource(resource={"path": "P1/Sub/MyMixCaseText2.txt", "content": {"value": "some text"}, "title": "My Text Sub Mixed Case Path Resource"}) }}
{{- add_resource(resource={"path": "p1/sub/data1.yaml", "content": resource_content, "title": "Sub data"}) }}
{%- set resource_params = {"data2ParaM1": "data2Param1v"} %}
{{- add_resource(resource={"path": "p1/data2.yaml", "name": "data2.yaml", "title": "My data 2", "params": resource_params, "content": resource_content}) }}
{{- add_resource(resource={"path": "p1/featuredimage.png", "name": "featured.png", "title": "My Featured Image", "params": resource_params, "content": {"value": pixel} }) }}
"#;

fn basic_files(draft: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "config.toml",
            "disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\nbaseURL = \"https://example.com\"\n".to_owned(),
        ),
        ("assets/a/pixel.png", String::new()),
        ("assets/mydata.yaml", format!("p1: \"p1\"\ndraft: {draft}\n")),
        (
            "layouts/_partials/get-value.html",
            "{{ return_value(value=\"p1\") }}".to_owned(),
        ),
        ("layouts/single.html", BASIC_SINGLE.to_owned()),
        ("layouts/list.html", BASIC_LIST.to_owned()),
        (
            "content/docs/pfile.md",
            "---\ntitle: \"pfile\"\ndate: 2023-03-01\n---\nPfile Content\n".to_owned(),
        ),
        ("content/docs/_content.html", BASIC_ADAPTER.to_owned()),
    ]
}

fn borrow<'a>(files: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    files.iter().map(|(p, c)| (*p, c.as_str())).collect()
}

/// `TestPagesFromGoTmplMisc`: pages and resources, partials, data and `get_asset` in the
/// adapter.
#[test]
fn pages_and_resources() {
    let files = basic_files("false");
    let (_tmp, r) = build_ok(&borrow(&files));
    assert_contains(
        &text(&r, "docs/pfile/index.html"),
        &["Dates: Date: 2023-03-01|Lastmod: 2023-03-01|PublishDate: 2023-03-01|ExpiryDate: |"],
    );
    let p1 = text(&r, "docs/p1/index.html");
    assert_contains(
        &p1,
        &[
            "Single: p1:p1|",
            "Path: /docs/p1|",
            "<strong>Hello World</strong>",
            "Params: param1v|",
            "Len Resources: 7",
            r#"RelPermalink: /mydata.yaml|Name: data1.yaml|Title: data1.yaml|Params: {}|"#,
            r#"RelPermalink: /mydata.yaml|Name: data2.yaml|Title: My data 2|Params: {"data2param1":"data2Param1v"}|"#,
            r#"RelPermalink: /a/pixel.png|Name: featured.png|Title: My Featured Image|Params: {"data2param1":"data2Param1v"}|"#,
            "RelPermalink: /docs/p1/sub/mytex2.txt|Name: sub/mytex2.txt|",
            "RelPermalink: /docs/p1/sub/mymixcasetext2.txt|Name: sub/mymixcasetext2.txt|",
            r#"RelPermalink: /mydata.yaml|Name: sub/data1.yaml|Title: Sub data|Params: {}|"#,
            "Featured Image: /a/pixel.png|featured.png|",
            "Resized Featured Image: 10|",
            r#"RelPermalink: /docs/p1/mytext.txt|Name: textresource|Title: My Text Resource|Params: {"param1":"param1v"}|"#,
            "Dates: Date: 2023-03-01|Lastmod: 2023-03-01|PublishDate: 2023-03-01|ExpiryDate: |",
        ],
    );
    assert_contains(
        &text(&r, "docs/p2/index.html"),
        &[
            "Single: p2title|",
            "<b>Hello World!</b> No **markdown** here.",
        ],
    );
    assert_contains(
        &text(&r, "docs/p3/index.html"),
        &["<strong>Hello World Default</strong>"],
    );
    // The resources the adapter made from text are published below the page.
    for f in [
        "docs/p1/mytext.txt",
        "docs/p1/sub/mytex2.txt",
        "docs/p1/sub/mymixcasetext2.txt",
    ] {
        assert!(exists(&r, f), "{f}");
    }
    assert_eq!(
        r.memory
            .as_ref()
            .expect("memory")
            .text("docs/p1/mytext.txt")
            .as_deref(),
        Some("some text")
    );
    assert_contains(
        &text(&r, "index.html"),
        &[
            "RegularPagesRecursive: p1:p1:/docs/p1|p2title:/docs/p2|p3title:/docs/p3|p4title:/docs/p4|pfile:/docs/pfile|$",
            "Sections: Docs:/docs|$",
        ],
    );
}

/// `TestPagesFromGoTmplDraftFlagFromResource`: a draft from the adapter's data.
#[test]
fn draft_pages_are_left_out() {
    let files = basic_files("true");
    let (_tmp, r) = build_ok(&borrow(&files));
    assert_contains(
        &text(&r, "index.html"),
        &[
            "RegularPagesRecursive: p1:p1:/docs/p1|p2title:/docs/p2|p3title:/docs/p3|pfile:/docs/pfile|$",
        ],
    );
    assert!(!exists(&r, "docs/p4/index.html"));
}

/// `TestPagesFromGoTmplAsciidocAndSimilar` (Markdown and HTML only) and a page without
/// content.
#[test]
fn page_without_content() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\nbaseURL = \"https://example.com\"\n",
        ),
        (
            "layouts/single.html",
            "|Content: {{ page.content }}|Title: {{ page.title }}|Path: {{ page.path }}|",
        ),
        ("layouts/list.html", "list"),
        (
            "content/docs/_content.html",
            r#"{{ add_page(page={"path": "nocontent", "title": "No Content"}) }}"#,
        ),
    ]);
    assert_contains(
        &text(&r, "docs/nocontent/index.html"),
        &["|Content: |Title: No Content|Path: /docs/nocontent|"],
    );
}

/// `TestPagesFromGoTmplAddPageErrors`: the errors name the adapter and the call.
#[test]
fn add_page_errors() {
    for (page, want) in [
        (r#"{"kind": "page", "title": "p1"}"#, "`path` is empty"),
        (
            r#"{"kind": "page", "path": "p1", "lang": "en"}"#,
            "`lang` cannot be set",
        ),
        (
            r#"{"path": "p1", "content": {"markup": "md"} }"#,
            "`content.markup` cannot be set",
        ),
        (
            r#"{"path": "p1", "cascade": {"params": {"a": 1} } }"#,
            "only branch pages can cascade",
        ),
        (
            r#"{"path": "p1", "content": {"mediaType": "text/nope"} }"#,
            "text/nope",
        ),
        // Go adds a page of kind "Page" that is never rendered.
        (r#"{"path": "p1", "kind": "Page"}"#, "is not one of page"),
    ] {
        let adapter = format!("{{{{ add_page(page={page}) }}}}");
        let (_tmp, r) = try_build(&[
            ("config.toml", "baseURL = \"https://example.com\"\n"),
            ("layouts/single.html", "single"),
            ("layouts/list.html", "list"),
            ("content/docs/_content.html", &adapter),
        ]);
        let e = format!("{:#}", anyhow_chain(&r.expect_err(page)));
        assert!(e.contains("_content.html"), "{e}");
        assert!(e.contains("add_page"), "{e}");
        assert!(e.contains(want), "{want:?} not in {e}");
    }
}

/// An error and its sources, one per line.
fn anyhow_chain(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut cur = e.source();
    while let Some(s) = cur {
        out.push_str("\n  ");
        out.push_str(&s.to_string());
        cur = s.source();
    }
    out
}

/// `TestPagesFromGoTmplAddPageErrors` (site methods not ready): `site` has no page lists, and
/// the adapter functions are not available in layouts.
#[test]
fn site_lists_and_functions_outside_adapters() {
    let (_tmp, r) = try_build(&[
        ("config.toml", "baseURL = \"https://example.com\"\n"),
        ("layouts/single.html", "single"),
        ("layouts/list.html", "list"),
        (
            "content/docs/_content.html",
            "{{ site.regular_pages | length }}",
        ),
    ]);
    let e = anyhow_chain(&r.expect_err("site.regular_pages"));
    assert!(e.contains("regular_pages"), "{e}");

    let (_tmp, r) = try_build(&[
        ("config.toml", "baseURL = \"https://example.com\"\n"),
        (
            "layouts/home.html",
            "{{ add_page(page={\"path\": \"x\"}) }}",
        ),
        ("content/_index.md", "---\ntitle: Home\n---\n"),
    ]);
    let e = anyhow_chain(&r.expect_err("add_page in a layout"));
    assert!(e.contains("only available in content adapters"), "{e}");
}

/// A Go-template adapter, and Go syntax in a Tera one, are errors with a hint.
#[test]
fn go_template_adapters_are_refused() {
    let (_tmp, r) = try_build(&[
        ("config.toml", "baseURL = \"https://example.com\"\n"),
        ("layouts/list.html", "list"),
        (
            "content/docs/_content.gotmpl",
            "{{ $.AddPage (dict \"path\" \"p1\") }}",
        ),
    ]);
    let e = anyhow_chain(&r.expect_err("gotmpl"));
    assert!(e.contains("_content.gotmpl"), "{e}");
    assert!(e.contains("_content.html"), "{e}");

    let (_tmp, r) = try_build(&[
        ("config.toml", "baseURL = \"https://example.com\"\n"),
        ("layouts/list.html", "list"),
        (
            "content/docs/_content.html",
            "{{ $.AddPage (dict \"path\" \"p1\") }}",
        ),
    ]);
    let e = anyhow_chain(&r.expect_err("Go syntax"));
    assert!(e.contains("Go template syntax"), "{e}");
}
