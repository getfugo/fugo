//! `get_page`, `deref`, `get_terms`, `related`, `param`, the page store, menus; components via
//! `page=` and via `@__nh`.

use ssg_base::PageKind;
use ssg_view::Phase;

use crate::support;

#[test]
fn get_page_cases() {
    let site = support::load();
    let home = site.home_scope();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    let r = |src: &str, s| site.render(src, s);
    // Absolute paths, with or without extension or trailing slash; the full value.
    assert_eq!(
        r(
            "{{ get_page(path=\"/posts\") | get_path(path=[\"title\"]) }}",
            &home
        ),
        "Posts"
    );
    assert_eq!(
        r(
            "{{ get_page(path=\"/posts/\") | get_path(path=[\"title\"]) }}",
            &home
        ),
        "Posts"
    );
    assert_eq!(
        r(
            "{{ get_page(path=\"/posts/two.md\") | get_path(path=[\"title\"]) }}",
            &home
        ),
        "Two"
    );
    assert_eq!(
        r(
            "{{ get_page(path=\"/posts\") | get_path(path=[\"regular_pages\"]) | length }}",
            &home
        ),
        "4"
    );
    // Missing: none.
    assert_eq!(r("{{ get_page(path=\"/nope\") is none }}", &home), "true");
    // Another language, by `lang=`, and the language of the render.
    assert_eq!(
        r(
            "{{ get_page(path=\"/posts/one\", lang=\"fr\") | get_path(path=[\"title\"]) }}",
            &home
        ),
        "Un"
    );
    let fr_home = site.scope(site.page(PageKind::Home, "/", 1), None);
    assert_eq!(
        r(
            "{{ get_page(path=\"/posts/one\") | get_path(path=[\"title\"]) }}",
            &fr_home
        ),
        "Un"
    );
    // Relative paths resolve against the render's page, or `page=`.
    assert_eq!(
        r(
            "{{ get_page(path=\"./two\") | get_path(path=[\"title\"]) }}",
            &one
        ),
        "Two"
    );
    assert_eq!(
        r(
            "{{ get_page(path=\"../about\") | get_path(path=[\"title\"]) }}",
            &one
        ),
        "About"
    );
    assert_eq!(
        r(
            "{{ get_page(path=\"two\", page=get_page(path=\"/posts/one\")) | get_path(path=[\"title\"]) }}",
            &home
        ),
        "Two"
    );
    // Page names resolve without a page (site.GetPage).
    assert_eq!(
        r(
            "{{ get_page(path=\"about\") | get_path(path=[\"title\"]) }}",
            &one
        ),
        "About"
    );
    // An unknown language is an error.
    let e = site
        .try_render("{{ get_page(path=\"/\", lang=\"xx\") }}", &home)
        .expect_err("unknown language");
    assert!(e.to_string().contains("no such language"), "{e}");
}

#[test]
fn deref_gives_the_full_value_of_a_listed_page() {
    let site = support::load();
    let home = site.home_scope();
    // Listed pages are summaries (no relations); deref adds them.
    assert_eq!(
        site.render(
            "{% for p in site.regular_pages %}{% if p.title == \"One\" %}{{ p | deref | get_path(path=[\"parent\", \"title\"]) }}{% endif %}{% endfor %}",
            &home
        ),
        "Posts"
    );
    let e = site
        .try_render("{{ get_asset(path=\"css/a.css\") | deref }}", &home)
        .expect_err("a resource is not a page");
    assert!(
        e.to_string().contains("expected a page, got a resource"),
        "{e}"
    );
}

#[test]
fn get_terms_and_param() {
    let site = support::load();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    let home = site.home_scope();
    assert_eq!(
        site.render(
            "{{ [q.title for q in get_terms(taxonomy=\"tags\")] | join(sep=\",\") }}",
            &one
        ),
        "A,B"
    );
    assert_eq!(
        site.render("{{ get_terms(taxonomy=\"keywords\") | length }}", &one),
        "0"
    );
    assert_eq!(site.render("{{ param(key=\"color\") }}", &one), "red");
    assert_eq!(
        site.render("{{ param(key=\"nested.deep\") }}", &one),
        "page-deep"
    );
    assert_eq!(site.render("{{ param(key=\"color\") }}", &home), "blue");
    assert_eq!(
        site.render("{{ param(key=\"nested.deep\") }}", &home),
        "site-deep"
    );
    assert_eq!(
        site.render("{{ param(key=\"missing\") is none }}", &home),
        "true"
    );
    assert_eq!(
        site.render(
            "{{ param(key=\"color\", page=get_page(path=\"/posts/one\")) }}",
            &home
        ),
        "red"
    );
}

#[test]
fn related_uses_an_index_per_candidate_list() {
    let site = support::load();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    // one has tags a, b: two (a) and three (b) match; one itself never does.
    let out = site.render(
        "{{ [q.title for q in related(pages=site.regular_pages)] | join(sep=\",\") }}",
        &one,
    );
    let mut got: Vec<&str> = out.split(',').collect();
    got.sort_unstable();
    assert_eq!(got, ["Three", "Two"]);
    assert_eq!(
        site.render(
            "{{ related(pages=site.regular_pages, limit=1) | length }}",
            &one
        ),
        "1"
    );
    assert_eq!(
        site.render(
            "{{ [q.title for q in related(pages=[get_page(path=\"/posts/two\")])] | join(sep=\",\") }}",
            &one
        ),
        "Two"
    );
    let e = site
        .try_render(
            "{{ related(pages=site.regular_pages, indices=[\"nope\"]) }}",
            &one,
        )
        .expect_err("unknown index");
    assert!(e.to_string().contains("nope"), "{e}");
}

#[test]
fn page_store_writes_are_direct_in_layouts_and_buffered_in_content() {
    let site = support::load();
    let one = site.page(PageKind::Page, "/posts/one", 0);
    let layout = site.scope(one, None);
    assert_eq!(
        site.render(
            "{{ store_set(key=\"k\", value=[1, 2]) }}{{ store_get(key=\"k\") | length }}",
            &layout
        ),
        "2"
    );
    // Another page's store through `page=`.
    let home = site.home_scope();
    assert_eq!(
        site.render(
            "{{ store_get(key=\"k\", page=get_page(path=\"/posts/one\")) | length }}|{{ store_get(key=\"k\") is none }}",
            &home
        ),
        "2|true"
    );
    // Content phase: buffered in the transaction until committed.
    let mut content = layout.clone();
    content.phase = Phase::Content;
    let txn = site.handles.stores.begin();
    content.txn = Some(txn);
    assert_eq!(
        site.render(
            "{{ store_set(key=\"c\", value=\"v\") }}{{ store_get(key=\"c\") }}",
            &content
        ),
        "v"
    );
    assert_eq!(
        site.render("{{ store_get(key=\"c\") is none }}", &layout),
        "true"
    );
    site.handles.stores.commit(txn);
    assert_eq!(site.render("{{ store_get(key=\"c\") }}", &layout), "v");
}

#[test]
fn menu_current_queries() {
    let site = support::load();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    let posts = site.scope(site.page(PageKind::Section, "/posts", 0), None);
    let src = "{% for e in site.menus.main %}{{ e.name }}:{{ is_menu_current(menu=\"main\", entry=e) }}/{{ has_menu_current(menu=\"main\", entry=e) }};{% endfor %}";
    assert_eq!(site.render(src, &one), "Posts:false/true;");
    assert_eq!(site.render(src, &posts), "Posts:true/false;");
    let child = "{{ is_menu_current(menu=\"main\", entry=site.menus.main[0].children[0]) }}";
    assert_eq!(site.render(child, &one), "true");
}

#[test]
fn components_reach_the_scope_through_page_or_at_nh() {
    let site = support::load();
    let home = site.home_scope();
    // `page=` (declared as `@page`): i18n in that page's language.
    assert_eq!(site.render("{{ <tr key=\"hello\" /> }}", &home), "Hello");
    let fr = site.scope(site.page(PageKind::Home, "/", 1), None);
    assert_eq!(site.render("{{ <tr key=\"hello\" /> }}", &fr), "Bonjour");
    // `@__nh`: paginator sees the render scope.
    assert_eq!(site.render("{{ <pg /> }}", &home), "1");
    // Neither: an error with a hint.
    let e = site
        .try_render("{{ <bad key=\"hello\" /> }}", &home)
        .expect_err("no scope in the component");
    let e = e.to_string();
    assert!(e.contains("pass `page=`") && e.contains("@__nh"), "{e}");
}
