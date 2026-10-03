//! `partial`, frames and `return_value`, `partial_cached`, `template_exists`, `defer`.

use ssg_base::PageKind;
use ssg_view::Phase;

use crate::support;

#[test]
fn partial_renders_with_kwargs_and_the_callers_names() {
    let site = support::load();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    // Safe in HTML: not escaped again.
    assert_eq!(
        site.render("{{ partial(name=\"text\", word=\"hi\") }}", &one),
        "<b>hi</b>|One|en"
    );
    assert_eq!(
        site.render("{{ partial(name=\"text.html\", word=\"<x>\") }}", &one),
        "<b>&lt;x&gt;</b>|One|en"
    );
    // A kwarg shadows an inherited name.
    assert_eq!(
        site.render(
            "{{ partial(name=\"text\", word=\"w\", page=get_page(path=\"/about\")) }}",
            &one
        ),
        "<b>w</b>|About|en"
    );
    let e = site
        .try_render("{{ partial(name=\"nope\") }}", &one)
        .expect_err("missing partial");
    assert!(e.to_string().contains("no such partial"), "{e}");
}

#[test]
fn nested_return_values_use_their_own_frames() {
    let site = support::load();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    assert_eq!(
        site.render(
            "{% set r = partial(name=\"inner\", n=1) %}{{ r.n }}-{{ r.from }}",
            &one
        ),
        "1-One"
    );
    // outer calls inner (n + 1) and returns a list built from inner's value and its own n.
    assert_eq!(
        site.render("{{ partial(name=\"outer\", n=5) | join(sep=\",\") }}", &one),
        "6,One,5"
    );
    // Frames are closed after the call.
    let e = site
        .try_render("{{ return_value(value=1) }}", &one)
        .expect_err("outside a partial");
    assert!(
        e.to_string()
            .contains("only allowed in a template rendered by `partial()`"),
        "{e}"
    );
}

#[test]
fn partial_cached_caches_values() {
    let site = support::load();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    // `count` increments the page store each time it renders.
    assert_eq!(
        site.render(
            "{% set a = partial_cached(name=\"count\", key=\"x\", k=1) %}{% set b = partial_cached(name=\"count\", key=\"x\", k=2) %}{% set c = partial_cached(name=\"count\", key=\"y\", k=3) %}{{ a.k }}{{ b.k }}{{ c.k }}|{{ store_get(key=\"calls\") }}",
            &one
        ),
        "113|2"
    );
    assert_eq!(
        site.render(
            "{% set a = partial(name=\"count\", k=1) %}{% set b = partial(name=\"count\", k=2) %}{{ a.k }}{{ b.k }}|{{ store_get(key=\"calls\") }}",
            &one
        ),
        "12|4"
    );
}

#[test]
fn template_exists_by_name_or_path() {
    let site = support::load();
    let s = site.home_scope();
    assert_eq!(
        site.render(
            "{{ template_exists(name=\"_partials/text.html\") }}{{ template_exists(name=\"_partials/none.html\") }}",
            &s
        ),
        "truefalse"
    );
}

#[test]
fn defer_registers_once_per_key_and_returns_a_placeholder() {
    let site = support::load();
    let s = site.home_scope();
    assert_eq!(
        site.render(
            "{{ defer(template=\"_partials/deferred/late.html\", key=\"k1\", data={\"x\": 1}) }}{{ defer(template=\"_partials/deferred/late.html\", key=\"k1\", data={\"x\": 2}) }}",
            &s
        ),
        "__nh_defer_k1____nh_defer_k1__"
    );
    let entries = site.handles.deferred.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(&*entries[0].1.template, "_partials/deferred/late.html");
    assert_eq!(entries[0].1.data.to_string(), "{\"x\": 1}");
    let e = site
        .try_render(
            "{{ defer(template=\"_partials/none.html\", key=\"k2\") }}",
            &s,
        )
        .expect_err("missing template");
    assert!(e.to_string().contains("no such template"), "{e}");
    let mut content = s.clone();
    content.phase = Phase::Content;
    assert!(
        site.try_render(
            "{{ defer(template=\"_partials/deferred/late.html\", key=\"k3\") }}",
            &content
        )
        .is_err()
    );
}
