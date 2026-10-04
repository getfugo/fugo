//! Adapters per language, all languages and the store, the default sort, cascades and build
//! options.

use super::*;

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
