//! Paths with dots and parameter case, paths joined as Go joins them, media types, menus,
//! summaries, outputs, the home page and pages listed but not rendered.

use super::*;

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
