//! Content adapters (`content/**/_content.html`): Go's integration tests of
//! `pagesfromdata/pagesfromgotmpl_integration_test.go` with the adapters and layouts
//! converted to Tera, built in memory.

use std::fs;

use ssg_build::{BuildError, BuildReport, BuildRequest, SinkKind, build};
use ssg_config::CliOverrides;

use crate::support::write_files;

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

/// `TestPagesFromGoTmplLanguagePerFile`: one adapter per language file; a disabled language's
/// adapter does not run.
#[test]
fn adapter_per_language() {
    for disabled in [false, true] {
        let config = format!(
            "defaultContentLanguage = \"en\"\ndefaultContentLanguageInSubdir = true\n\
             [languages]\n[languages.en]\nweight = 1\ntitle = \"Title\"\n\
             [languages.fr]\nweight = 2\ntitle = \"Titre\"\ndisabled = {disabled}\n"
        );
        let (_tmp, r) = build_ok(&[
            ("config.toml", &config),
            (
                "layouts/single.html",
                "Single: {{ page.title }}|{{ page.content }}|",
            ),
            ("layouts/list.html", "list"),
            (
                "content/docs/_content.html",
                r#"{{ add_page(page={"kind": "page", "path": "p1", "title": "Title"}) }}"#,
            ),
            (
                "content/docs/_content.fr.html",
                r#"{{ add_page(page={"kind": "page", "path": "p1", "title": "Titre"}) }}"#,
            ),
        ]);
        assert_contains(&text(&r, "en/docs/p1/index.html"), &["Single: Title||"]);
        assert_eq!(exists(&r, "fr/docs/p1/index.html"), !disabled, "{disabled}");
        if !disabled {
            assert_contains(&text(&r, "fr/docs/p1/index.html"), &["Single: Titre||"]);
        }
    }
}

/// `TestPagesFromGoTmplEnableAllLanguages`: the adapter runs for every language and its runs
/// share the store.
#[test]
fn enable_all_languages_and_the_store() {
    for disabled in [false, true] {
        let config = format!(
            "defaultContentLanguage = \"en\"\ndefaultContentLanguageInSubdir = true\n\
             [languages]\n[languages.en]\nweight = 1\ntitle = \"Title\"\n\
             [languages.fr]\ntitle = \"Titre\"\nweight = 2\ndisabled = {disabled}\n"
        );
        let (_tmp, r) = build_ok(&[
            ("config.toml", &config),
            ("i18n/en.yaml", "title: Title\n"),
            ("i18n/fr.yaml", "title: Titre\n"),
            (
                "content/docs/_content.html",
                r#"{{- enable_all_languages() }}
{%- set title_from_store = store_get(key="title") %}
{%- if not title_from_store %}
  {%- set title_from_store = "notfound" %}
  {{- store_set(key="title", value=site.title) }}
{%- endif %}
{%- set title = site.title ~ ":" ~ i18n(key="title") ~ ":" ~ title_from_store %}
{{- add_page(page={"kind": "page", "path": "p1", "title": title}) }}"#,
            ),
            (
                "layouts/single.html",
                "Single: {{ page.title }}|{{ page.content }}|",
            ),
            ("layouts/list.html", "list"),
        ]);
        assert_eq!(exists(&r, "fr/docs/p1/index.html"), !disabled, "{disabled}");
        if !disabled {
            assert_contains(
                &text(&r, "en/docs/p1/index.html"),
                &["Single: Title:Title:notfound||"],
            );
            assert_contains(
                &text(&r, "fr/docs/p1/index.html"),
                &["Single: Titre:Titre:Title||"],
            );
        }
    }
}

/// `TestPagesFromGoTmplDefaultPageSort`: pages without weight, date or distinct titles sort
/// by their path.
#[test]
fn default_page_sort() {
    let (_tmp, r) = build_ok(&[
        ("config.toml", "defaultContentLanguage = \"en\"\n"),
        (
            "layouts/home.html",
            "{% for p in site.regular_pages %}{{ p.rel_permalink }}|{% endfor %}",
        ),
        ("layouts/single.html", "single"),
        ("layouts/list.html", "list"),
        (
            "content/_content.html",
            r#"{{ add_page(page={"kind": "page", "path": "docs/_p22", "title": "A"}) }}
{{ add_page(page={"kind": "page", "path": "docs/p12", "title": "A"}) }}
{{ add_page(page={"kind": "page", "path": "docs/_p12", "title": "A"}) }}"#,
        ),
        (
            "content/docs/_content.html",
            r#"{{ add_page(page={"kind": "page", "path": "_p21", "title": "A"}) }}
{{ add_page(page={"kind": "page", "path": "p11", "title": "A"}) }}
{{ add_page(page={"kind": "page", "path": "_p11", "title": "A"}) }}"#,
        ),
    ]);
    assert_contains(
        &text(&r, "index.html"),
        &["/docs/_p11/|/docs/_p12/|/docs/_p21/|/docs/_p22/|/docs/p11/|/docs/p12/|"],
    );
}

/// `TestPagesFromGoTmplCascade` and `TestContentAdapterCascadeBasic`: an adapter section's
/// cascade, and a content file's cascade of fields onto adapter pages.
#[test]
fn cascade() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\nbaseURL = \"https://example.com\"\n",
        ),
        (
            "layouts/single.html",
            "|Content: {{ page.content }}|Title: {{ page.title }}|Path: {{ page.path }}|Params: {{ page.params | jsonify }}|",
        ),
        ("layouts/list.html", "list"),
        (
            "content/_content.html",
            r#"{%- set cascade = {"params": {"cascadeparam1": "cascadeparam1value"} } %}
{{- add_page(page={"path": "docs", "kind": "section", "cascade": cascade}) }}
{{- add_page(page={"path": "docs/p1", "content": {"value": "**Hello World**", "mediaType": "text/markdown"} }) }}"#,
        ),
    ]);
    assert_contains(
        &text(&r, "docs/p1/index.html"),
        &[r#"|Path: /docs/p1|Params: {"cascadeparam1":"cascadeparam1value"}|"#],
    );

    let (_tmp, r) = build_ok(&[
        ("config.toml", "disableLiveReload = true\n"),
        (
            "content/_index.md",
            "---\ncascade:\n  - title: foo\n    target:\n      path: \"**\"\n---\n",
        ),
        (
            "layouts/all.html",
            "Title: {{ page.title }}|Content: {{ page.content }}|",
        ),
        (
            "content/_content.html",
            r#"{%- set content = {"mediaType": "text/markdown", "value": "The _Hunchback of Notre Dame_ was published in 1831."} %}
{{- add_page(page={"path": "s1", "kind": "page"}) }}
{{- add_page(page={"path": "s2", "kind": "page", "title": "bar", "content": content}) }}"#,
        ),
    ]);
    assert_contains(&text(&r, "s1/index.html"), &["Title: foo|"]);
    assert_contains(
        &text(&r, "s2/index.html"),
        &[
            "Title: bar|",
            "Content: <p>The <em>Hunchback of Notre Dame</em> was published in 1831.</p>",
        ],
    );
}

/// `TestPagesFromGoBuildOptions`: `build.render = never`.
#[test]
fn build_options() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\nbaseURL = \"https://example.com\"\n",
        ),
        ("layouts/single.html", "|Title: {{ page.title }}|"),
        ("layouts/list.html", "list"),
        (
            "content/_content.html",
            r#"{{- add_page(page={"path": "docs/p1", "content": {"value": "**Hello World**", "mediaType": "text/markdown"} }) }}
{%- set never = {"list": "never", "publishResources": false, "render": "never"} %}
{{- add_page(page={"path": "docs/p2", "content": {"value": "**Hello World**", "mediaType": "text/markdown"}, "build": never}) }}"#,
        ),
    ]);
    assert!(exists(&r, "docs/p1/index.html"));
    assert!(!exists(&r, "docs/p2/index.html"));
}

/// `TestPagesFromGoPathsWithDotsIssue12493` and `TestPagesFromGoParamsIssue12497`.
#[test]
fn paths_with_dots_and_param_case() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = ['home','section','rss','sitemap','taxonomy','term']\n",
        ),
        (
            "content/_content.html",
            r#"{{- add_page(page={"path": "s-1.2.3/p-4.5.6", "title": "p-4.5.6"}) }}
{{- add_page(page={"path": "p1", "title": "p1", "params": {"paraM1": "param1v"} }) }}
{{- add_resource(resource={"path": "p1/data1.yaml", "content": {"value": "data1"}, "params": {"paraM1": "param1v"} }) }}"#,
        ),
        (
            "layouts/single.html",
            "{{ page.title }}|{{ page.params.param1 or \"\" }}\n{% for r in page.resources %}{{ r.name }}|{{ r.params.param1 }}\n{% endfor %}",
        ),
    ]);
    assert!(exists(&r, "s-1.2.3/p-4.5.6/index.html"));
    assert_contains(
        &text(&r, "p1/index.html"),
        &["p1|param1v", "data1.yaml|param1v"],
    );
}

/// Paths as Go joins them (the output of the Go binary for the same adapter): a page path
/// loses one leading `/`, a resource path none (`path.Join` drops the rest); nothing is
/// trimmed, and spaces and tabs become `-`.
#[test]
fn paths_as_go_joins_them() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\nbaseURL = \"https://example.com/\"\n",
        ),
        ("content/books/_index.md", "---\ntitle: Books\n---\n"),
        (
            "content/books/_content.html",
            "{{ add_page(page={\"path\": \" spaced \", \"title\": \"Spaced\"}) }}
{{ add_page(page={\"path\": \"//double\", \"title\": \"Double\"}) }}
{{ add_page(page={\"path\": \"/single\", \"title\": \"Single\"}) }}
{{ add_page(page={\"path\": \"Tab\tName\", \"title\": \"Tab\"}) }}
{{ add_resource(resource={\"path\": \" lead.txt\", \"content\": {\"value\": \"l\"} }) }}
{{ add_resource(resource={\"path\": \"/abs/x.txt\", \"content\": {\"value\": \"a\"} }) }}
{{ add_resource(resource={\"path\": \"//two/y.txt\", \"content\": {\"value\": \"t\"} }) }}",
        ),
        (
            "layouts/single.html",
            "{{ page.title }}|{{ page.rel_permalink }}\n",
        ),
        (
            "layouts/list.html",
            "{{ page.title }}|P:{% for p in page.pages %}{{ p.title }}@{{ p.rel_permalink }};{% endfor %}|R:{% for r in page.resources %}{{ r.name }}@{{ r.rel_permalink }};{% endfor %}\n",
        ),
    ]);
    assert_eq!(
        text(&r, "books/index.html"),
        "Books|P:Double@/books/double/;Single@/books/single/;Spaced@/books/-spaced-/;Tab@/books/tab-name/;|R:-lead.txt@/books/-lead.txt;abs/x.txt@/books/abs/x.txt;two/y.txt@/books/two/y.txt;\n"
    );
}

/// `TestPagesFromGoTmplResourceWithoutExtensionWithMediaTypeProvided`.
#[test]
fn resource_media_type() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\nbaseURL = \"https://example.com\"\n",
        ),
        (
            "layouts/single.html",
            "{% for r in page.resources %}|RelPermalink: {{ r.rel_permalink }}|Name: {{ r.name }}|Title: {{ r.title }}|MediaType: {{ r.media_type.type }}|{% endfor %}",
        ),
        ("layouts/list.html", "list"),
        (
            "content/docs/_content.html",
            r#"{{- add_page(page={"path": "p1", "content": {"value": "**Hello World**", "mediaType": "text/markdown"} }) }}
{{- add_resource(resource={"path": "p1/myresource", "content": {"value": "abcde", "mediaType": "text/plain"} }) }}"#,
        ),
    ]);
    assert_contains(
        &text(&r, "docs/p1/index.html"),
        &[
            "|RelPermalink: /docs/p1/myresource|Name: myresource|Title: myresource|MediaType: text/plain|",
        ],
    );
}

/// `TestPagesFromGoTmplMenus` and `TestPagesFromGoTmplMenusMap`.
#[test]
fn menus() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = ['rss','section','sitemap','taxonomy','term']\n\
             [menus]\n[[menus.main]]\nname = \"Main\"\n[[menus.footer]]\nname = \"Footer\"\n",
        ),
        (
            "content/_content.html",
            r#"{{- add_page(page={"path": "p1", "title": "p1", "menus": "main"}) }}
{{- add_page(page={"path": "p2", "title": "p2", "menus": ["main", "footer"]}) }}
{{- add_page(page={"path": "p3", "title": "p3", "menus": {"m1": {"identifier": "id1"} } }) }}"#,
        ),
        (
            "layouts/home.html",
            "Main: {% for e in site.menus.main %}{{ e.name }}|{% endfor %}|\nFooter: {% for e in site.menus.footer %}{{ e.name }}|{% endfor %}|\nMenus: {% for k, v in site.menus | sort_keys %}{{ k }}|{% endfor %}",
        ),
        ("layouts/single.html", "single"),
    ]);
    assert_contains(
        &text(&r, "index.html"),
        &[
            "Main: Main|p1|p2||",
            "Footer: Footer|p2||",
            "Menus: footer|m1|main|",
        ],
    );
}

/// `TestPagesFromGoTmplMore`: a summary divider in the content.
#[test]
fn summary_divider() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = ['home','rss','section','sitemap','taxonomy','term']\n[markup.goldmark.renderer]\nunsafe = true\n",
        ),
        (
            "content/s1/_content.html",
            r#"{{- add_page(page={"content": {"mediaType": "text/markdown", "value": "aaa <!--more--> bbb"}, "title": "p1", "path": "p1"}) }}"#,
        ),
        (
            "layouts/single.html",
            "summary: {{ page.summary }}|content: {{ page.content }}",
        ),
    ]);
    assert_contains(
        &text(&r, "s1/p1/index.html"),
        &["<p>aaa</p>|content: <p>aaa</p>\n<p>bbb</p>"],
    );
}

/// `TestContentAdapterOutputsIssue13689` and `TestContentAdapterOutputsIssue13692`.
#[test]
fn outputs() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = ['home','rss','section','sitemap','taxonomy','term']\n[outputs]\npage = ['html','json']\n",
        ),
        ("layouts/page.html", "html: {{ page.title }}"),
        ("layouts/page.json", "json: {{ page.title }}"),
        ("content/p1.md", "---\ntitle: p1\n---\n"),
        ("content/p2.md", "---\ntitle: p2\noutputs:\n  - html\n---\n"),
        (
            "content/_content.html",
            r#"{{- add_page(page={"path": "p3", "title": "p3"}) }}
{{- add_page(page={"path": "p4", "title": "p4", "outputs": ["html"]}) }}"#,
        ),
    ]);
    for (f, want) in [
        ("p1/index.html", true),
        ("p1/index.json", true),
        ("p2/index.html", true),
        ("p2/index.json", false),
        ("p3/index.html", true),
        ("p3/index.json", true),
        ("p4/index.html", true),
        ("p4/index.json", false),
    ] {
        assert_eq!(exists(&r, f), want, "{f}");
    }

    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = ['page','home','sitemap','taxonomy','term']\n\
             [[cascade]]\noutputs = ['html','json']\n[cascade.target]\npath = '{/s2,/s4}'\n",
        ),
        ("layouts/section.html", "html: {{ page.title }}"),
        ("layouts/section.json", "json: {{ page.title }}"),
        ("layouts/section.rss.xml", "rss: {{ page.title }}"),
        ("content/s1/_index.md", "---\ntitle: s1\n---\n"),
        ("content/s2/_index.md", "---\ntitle: s2\n---\n"),
        (
            "content/_content.html",
            r#"{{- add_page(page={"path": "s3", "title": "s3", "kind": "section"}) }}
{{- add_page(page={"path": "s4", "title": "s4", "kind": "section"}) }}
{{- add_page(page={"path": "s5", "title": "s5", "kind": "section", "outputs": ["html"]}) }}"#,
        ),
    ]);
    for (f, want) in [
        ("s1/index.html", true),
        ("s1/index.json", false),
        ("s1/index.xml", true),
        ("s2/index.html", true),
        ("s2/index.json", true),
        ("s2/index.xml", false),
        ("s3/index.html", true),
        ("s3/index.json", false),
        ("s3/index.xml", true),
        ("s4/index.html", true),
        ("s4/index.json", true),
        ("s4/index.xml", false),
        ("s5/index.html", true),
        ("s5/index.json", false),
        ("s5/index.xml", false),
    ] {
        assert_eq!(exists(&r, f), want, "{f}");
    }
}

/// `TestPagesFromGoTmplHome`: the home page from an adapter.
#[test]
fn home_page() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\nbaseURL = \"https://example.com\"\n",
        ),
        ("layouts/all.html", "{{ page.kind }}: {{ page.title }}|"),
        (
            "content/_content.html",
            r#"{{ add_page(page={"title": "My Home!", "kind": "home"}) }}"#,
        ),
    ]);
    assert_contains(&text(&r, "index.html"), &["home: My Home!|"]);
}

/// The news section of the documentation site: release pages listed locally, not rendered,
/// with their publish dates, an external permalink and the section's `_index.md` next to the
/// adapter; RSS lists them, the sitemap does not.
#[test]
fn listed_but_not_rendered() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "baseURL = \"https://example.org/\"\n\
             [frontmatter]\ndate = ['date']\npublishDate = ['publishdate', 'date']\n\
             lastmod = [':git', 'lastmod', 'publishdate', 'date']\n\
             [[cascade]]\n[cascade.params]\nshow_publish_date = true\n\
             [cascade.target]\nkind = 'page'\npath = '{/news/**}'\n",
        ),
        (
            "content/news/_index.md",
            "---\ntitle: News\noutputs: [html, rss]\n---\n",
        ),
        (
            "content/news/_content.html",
            r#"{%- for r in [{"name": "v0.2.0", "at": "2025-10-02T13:54:48Z"}, {"name": "v0.1.0", "at": "2025-09-25T11:44:59Z"}] %}
{{- add_page(page={
    "build": {"render": "never", "list": "local"},
    "content": {"mediaType": "text/markdown", "value": ""},
    "dates": {"publishDate": r.at | to_date},
    "kind": "page",
    "params": {"permalink": "https://example.com/releases/" ~ r.name},
    "path": r.name | replace(from=".", to="-"),
    "slug": r.name,
    "title": "Release " ~ r.name,
}) }}
{%- endfor %}"#,
        ),
        (
            "layouts/list.html",
            "{% for p in page.pages | by_publish_date | reverse %}{{ p.title }}|{{ p.params.permalink }}|{{ p.publish_date | date(format=\"%Y-%m-%dT%H:%M:%S%:z\") }}|{{ p.date | date(format=\"%Y\") }}|{{ p.params.show_publish_date }}|{{ p.rel_permalink }}|{{ p.params | length }}\n{% endfor %}",
        ),
        (
            "layouts/list.rss.xml",
            "{% for p in page.pages %}<item>{{ p.title }}|{{ p.lastmod | date(format=\"%Y-%m-%d\") }}</item>{% endfor %}",
        ),
        ("layouts/home.html", "{{ site.regular_pages | length }}"),
        ("layouts/single.html", "single"),
    ]);
    assert_eq!(
        text(&r, "news/index.html"),
        "Release v0.2.0|https://example.com/releases/v0.2.0|2025-10-02T13:54:48+00:00||true||2\n\
         Release v0.1.0|https://example.com/releases/v0.1.0|2025-09-25T11:44:59+00:00||true||2\n"
    );
    assert_eq!(
        text(&r, "news/index.xml"),
        "<item>Release v0.1.0|2025-09-25</item><item>Release v0.2.0|2025-10-02</item>",
        "the default order: no dates (`date = ['date']`), so by title"
    );
    assert_eq!(text(&r, "index.html"), "0", "listed locally only");
    assert!(!exists(&r, "news/v0.2.0/index.html"));
    assert!(!text(&r, "sitemap.xml").contains("v0.2.0"));
    assert!(
        r.diagnostics
            .iter()
            .all(|d| !d.message.contains("duplicate content path")),
        "{:?}",
        r.diagnostics
    );
}

/// What the Go binary at 44529028 gives for the same adapter: the slug as given
/// (front matter slugs lose their `-`, adapter slugs do not), the sitemap priority from zero
/// printed as Go prints a float (`0`, `1`), the dates chained as `[frontmatter]` says, and a
/// path added twice by one adapter is the last `add_page` (a resource the last
/// `add_resource`), with a warning.
#[test]
fn slugs_priorities_dates_and_paths_added_twice() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "baseURL = \"https://example.com/\"\n\
             disableKinds = [\"taxonomy\", \"term\", \"rss\", \"section\"]\n",
        ),
        (
            "content/_content.html",
            r#"{{ add_page(page={"path": "p1", "title": "p1", "slug": "-sl-"}) }}
{{ add_page(page={"path": "p2", "title": "p2", "slug": "--a b--", "sitemap": {"priority": 1, "changefreq": "daily"} }) }}
{{ add_page(page={"path": "p3", "title": "first", "dates": {"lastmod": "2024-01-05T00:00:00Z" | to_date} }) }}
{{ add_resource(resource={"path": "p3/r.txt", "content": {"value": "one"} }) }}
{{ add_page(page={"path": "P3", "title": "second", "kind": "page", "dates": {"lastmod": "2024-01-05T00:00:00Z" | to_date} }) }}
{{ add_resource(resource={"path": "p3/r.txt", "content": {"value": "two"} }) }}"#,
        ),
        (
            "layouts/home.html",
            "{% for p in site.regular_pages %}{{ p.title }}|{{ p.rel_permalink }}|{% if p.date %}{{ p.date | date(format=\"%Y-%m-%d\") }}{% endif %}|{% if p.publish_date %}{{ p.publish_date | date(format=\"%Y-%m-%d\") }}{% endif %}|{% for x in p.resources %}{{ x.name }}={{ x | resource_content }}{% endfor %}\n{% endfor %}",
        ),
        ("layouts/single.html", "{{ page.title }}"),
    ]);
    assert_eq!(
        text(&r, "index.html"),
        "second|/p3/|2024-01-05|2024-01-05|r.txt=two\np1|/-sl-/|||\np2|/--a-b--/|||\n"
    );
    assert_eq!(text(&r, "p3/r.txt"), "two");
    let sitemap: String = text(&r, "sitemap.xml").split_whitespace().collect();
    assert_contains(
        &sitemap,
        &[
            "<loc>https://example.com/-sl-/</loc><priority>0</priority>",
            "<loc>https://example.com/--a-b--/</loc><changefreq>daily</changefreq><priority>1</priority>",
        ],
    );
    let warnings: Vec<&str> = r
        .diagnostics
        .iter()
        .filter_map(|d| d.id.as_deref())
        .filter(|id| id.starts_with("duplicate-"))
        .collect();
    assert_eq!(
        warnings,
        ["duplicate-content-path", "duplicate-resource-path"]
    );
}

/// A path two adapters add is the later adapter's (the adapter of a subdirectory runs after
/// the one above it), and a path a content file has stays the file's, with warnings: the
/// Go binary at 44529028 with one collector worker (its worker multiplier variable set to 1)
/// gives the same pages and resources.
#[test]
fn two_adapters_on_one_path() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "baseURL = \"https://example.com/\"\n\
             disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\n",
        ),
        (
            "content/_content.html",
            r#"{{ add_page(page={"path": "books/x", "title": "X from root"}) }}
{{ add_resource(resource={"path": "books/x/r.txt", "content": {"value": "root r"}, "title": "R root"}) }}
{{ add_page(page={"path": "books/y", "title": "Y from root"}) }}"#,
        ),
        (
            "content/books/_content.html",
            r#"{{ add_page(page={"path": "x", "title": "X from books"}) }}
{{ add_resource(resource={"path": "x/r.txt", "content": {"value": "books r"}, "title": "R books"}) }}
{{ add_page(page={"path": "dup", "title": "Dup from adapter"}) }}"#,
        ),
        (
            "content/books/sub/_content.html",
            r#"{{ add_page(page={"path": "../y", "title": "Y from sub"}) }}"#,
        ),
        ("content/books/dup.md", "---\ntitle: Dup from file\n---\n"),
        (
            "layouts/home.html",
            "{% for p in site.regular_pages %}{{ p.title }}|{{ p.rel_permalink }}|{% for x in p.resources %}{{ x.title }}={{ x | resource_content }};{% endfor %}\n{% endfor %}",
        ),
        ("layouts/single.html", "{{ page.title }}"),
        ("layouts/list.html", "{{ page.title }}"),
    ]);
    assert_eq!(
        text(&r, "index.html"),
        "Dup from file|/books/dup/|\nX from books|/books/x/|R books=books r;\n\
         Y from sub|/books/y/|\n"
    );
    assert_eq!(text(&r, "books/x/r.txt"), "books r");
    // One diagnostic per id is kept (Go's `Warnidf`).
    let warnings: Vec<&str> = r
        .diagnostics
        .iter()
        .filter_map(|d| d.id.as_deref())
        .filter(|id| id.starts_with("duplicate-"))
        .collect();
    assert_eq!(
        warnings,
        ["duplicate-content-path", "duplicate-resource-path"]
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("the one that runs later wins")
                && d.message.contains("books/_content.html is used")),
        "{:?}",
        r.diagnostics
    );
}

/// Adapters exist to make many pages from data: one adapter adds 20,000 pages and 20,000
/// resources, then a few of the paths again. Each repeat replaces the earlier page (resource)
/// with a warning, a content file keeps its path, and a later adapter wins over this one.
/// `add_page`, `add_resource` and the model's assembly find an earlier item of a path through
/// maps of the paths: with scans of the items added so far (quadratic) the model phase of this
/// build took 13–16 s in a test build, with the maps 1.5 s; the bound only catches the former.
#[test]
fn many_pages_and_resources() {
    const N: usize = 20_000;
    let adapter = format!(
        r#"{{%- for i in range(end={N}) %}}
{{{{- add_page(page={{"path": "p" ~ i, "title": "t" ~ i, "build": {{"render": "never"}} }}) }}}}
{{{{- add_resource(resource={{"path": "p" ~ i ~ "/r.txt", "content": {{"value": "r" ~ i}} }}) }}}}
{{%- endfor %}}
{{{{- add_page(page={{"path": "P7", "title": "t7 again", "build": {{"render": "never"}} }}) }}}}
{{{{- add_page(page={{"path": "p{last}", "title": "last again", "build": {{"render": "never"}} }}) }}}}
{{{{- add_page(page={{"path": "p5", "title": "t5 again"}}) }}}}
{{{{- add_resource(resource={{"path": "p7/r.txt", "content": {{"value": "r7 again"}} }}) }}}}
{{{{- add_resource(resource={{"path": "p5/r.txt", "content": {{"value": "r5 again"}} }}) }}}}"#,
        last = N - 1
    );
    let list = format!(
        r#"{{%- if page.path == "/books" %}}{{{{ page.pages | length }}}} pages
{{%- for path in ["p0", "p5", "p7", "p8", "p{last}"] %}}
{{%- set p = get_page(path="/books/" ~ path) %}}
{{{{ path }}}}: {{{{ p.title }}}}|{{%- for x in p.resources %}}{{{{ x.name }}}}={{{{ x | resource_content }}}};{{%- endfor %}}
{{%- endfor %}}{{%- endif %}}"#,
        last = N - 1
    );
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "baseURL = \"https://example.com/\"\n\
             disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\n",
        ),
        ("content/books/_index.md", "---\ntitle: Books\n---\n"),
        ("content/books/_content.html", &adapter),
        (
            "content/books/sub/_content.html",
            r#"{{ add_page(page={"path": "../p8", "title": "t8 from sub", "build": {"render": "never"} }) }}
{{ add_resource(resource={"path": "../p8/r.txt", "content": {"value": "r8 from sub"} }) }}"#,
        ),
        (
            "content/books/p5/index.md",
            "---\ntitle: t5 from file\n---\n",
        ),
        ("content/books/p5/r.txt", "r5 from file"),
        ("layouts/list.html", &list),
        ("layouts/single.html", "{{ page.title }}"),
    ]);
    assert_eq!(
        text(&r, "books/index.html"),
        format!(
            "{N} pages\np0: t0|r.txt=r0;\np5: t5 from file|r.txt=r5 from file;\n\
             p7: t7 again|r.txt=r7 again;\np8: t8 from sub|r.txt=r8 from sub;\n\
             p{last}: last again|r.txt=r{last};",
            last = N - 1
        )
    );
    // Only the file's page is rendered; the adapter's `p5` (rendered) lost to it.
    assert_eq!(text(&r, "books/p5/index.html"), "t5 from file");
    assert!(!exists(&r, "books/p7/index.html"));
    // The report keeps one warning per id (Go's `Warnidf`).
    let warnings: Vec<&str> = r
        .diagnostics
        .iter()
        .filter_map(|d| d.id.as_deref())
        .filter(|id| id.starts_with("duplicate-"))
        .collect();
    assert_eq!(
        warnings,
        ["duplicate-content-path", "duplicate-resource-path"]
    );
    // The model phase runs the adapters and assembles the model.
    let model = r
        .timings
        .iter()
        .find_map(|&(lap, d)| (lap == "model").then_some(d))
        .expect("model lap");
    assert!(
        model < std::time::Duration::from_secs(8),
        "{N} pages and resources: the model phase took {model:?}"
    );
}
