//! URL filters, `ref`/`rel_ref`, `i18n`.

use ssg_base::PageKind;
use ssg_base::diag::Severity;

use crate::support;

#[test]
fn url_filters_use_the_render_language() {
    let site = support::load();
    let en = site.home_scope();
    let fr = site.scope(site.page(PageKind::Home, "/", 1), None);
    let src = "{{ \"/a/b\" | abs_url }}|{{ \"a\" | rel_url }}|{{ \"a\" | abs_lang_url }}|{{ \"a\" | rel_lang_url }}";
    assert_eq!(
        site.render(src, &en),
        "https://example.org/a/b|/sub/a|https://example.org/sub/a|/sub/a"
    );
    assert_eq!(
        site.render(src, &fr),
        "https://example.org/a/b|/sub/a|https://example.org/sub/fr/a|/sub/fr/a"
    );
    // `page=` picks the language of the prefix.
    assert_eq!(
        site.render(
            "{{ \"a\" | rel_lang_url(page=get_page(path=\"/\", lang=\"fr\")) }}",
            &en
        ),
        "/sub/fr/a"
    );
    // Without a scope: the context's `lang`.
    assert_eq!(
        site.render_bare("{{ \"a\" | rel_url }}").expect("bare"),
        "/sub/a"
    );
}

#[test]
fn ref_and_rel_ref_cases() {
    let site = support::load();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    let r = |src: &str| site.render(src, &one);
    assert_eq!(
        r("{{ ref(path=\"/posts/two\") }}"),
        "https://example.org/sub/posts/two/"
    );
    assert_eq!(
        r("{{ rel_ref(path=\"/posts/two.md#frag\") }}"),
        "/sub/posts/two/#frag"
    );
    // Relative to the render's page, and page names.
    assert_eq!(r("{{ rel_ref(path=\"two.md\") }}"), "/sub/posts/two/");
    assert_eq!(r("{{ rel_ref(path=\"about\") }}"), "/sub/about/");
    // Another language; a fragment alone.
    assert_eq!(
        r("{{ rel_ref(path=\"/posts/one\", lang=\"fr\") }}"),
        "/sub/fr/posts/one/"
    );
    assert_eq!(r("{{ rel_ref(path=\"#top\") }}"), "#top");
    // Another output format.
    assert_eq!(
        r("{{ rel_ref(path=\"/\", output_format=\"rss\") }}"),
        "/sub/index.xml"
    );
    // Not found: refLinksNotFoundURL and an error diagnostic (refLinksErrorLevel = ERROR).
    assert_eq!(r("{{ rel_ref(path=\"/nope\") }}"), "/404-ref/");
    let report = site.handles.diagnostics.report();
    assert!(
        report
            .iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("/nope")),
        "{report:?}"
    );
}

#[test]
fn i18n_in_the_page_language_with_counts() {
    let site = support::load();
    let en = site.home_scope();
    let fr = site.scope(site.page(PageKind::Home, "/", 1), None);
    assert_eq!(site.render("{{ i18n(key=\"hello\") }}", &en), "Hello");
    assert_eq!(site.render("{{ i18n(key=\"hello\") }}", &fr), "Bonjour");
    assert_eq!(
        site.render(
            "{{ i18n(key=\"hello\", page=get_page(path=\"/\", lang=\"fr\")) }}",
            &en
        ),
        "Bonjour"
    );
    assert_eq!(
        site.render("{{ i18n(key=\"posts\", count=1) }}", &en),
        "1 post"
    );
    assert_eq!(
        site.render("{{ i18n(key=\"posts\", count=3) }}", &en),
        "3 posts"
    );
    assert_eq!(
        site.render("{{ i18n(key=\"posts\", data=2) }}", &en),
        "2 posts"
    );
    // French lacks `posts`: the default language's text.
    assert_eq!(
        site.render("{{ i18n(key=\"posts\", count=2) }}", &fr),
        "2 posts"
    );
    assert_eq!(
        site.render_bare("{{ i18n(key=\"hello\") }}").expect("bare"),
        "Hello"
    );
}
