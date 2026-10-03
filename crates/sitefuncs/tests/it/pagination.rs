//! Pagination recorded at call time (REWRITE_PLAN.md §3.3).

use ssg_base::PageKind;
use ssg_view::Phase;

use crate::support;

#[test]
fn the_first_call_records_the_default_list() {
    let site = support::load();
    let home = site.page(PageKind::Home, "/", 0);
    let s = site.scope(home, None);
    assert!(site.handles.pagination.get(home, site.html()).is_none());
    // 5 regular pages, pager size 2.
    assert_eq!(
        site.render(
            "{{ paginator() | get_path(path=[\"page_number\"]) }}/{{ paginator() | get_path(path=[\"total_pages\"]) }}/{{ paginator() | get_path(path=[\"pages\"]) | length }}",
            &s
        ),
        "1/3/2"
    );
    let rec = site
        .handles
        .pagination
        .get(home, site.html())
        .expect("recorded");
    assert_eq!(rec.total_pages(), 3);
    assert_eq!(
        rec.first_call
            .as_ref()
            .expect("position")
            .file
            .to_string_lossy(),
        "layout of / (html)"
    );
    // The pager links.
    assert_eq!(
        site.render("{{ paginator() | get_path(path=[\"next\", \"url\"]) }}|{{ paginator() | get_path(path=[\"last\", \"url\"]) }}", &s),
        "/sub/page/2/|/sub/page/3/"
    );
}

#[test]
fn an_identical_recall_reuses_the_record() {
    let site = support::load();
    let posts = site.scope(site.page(PageKind::Section, "/posts", 0), None);
    assert_eq!(
        site.render(
            "{{ paginate(pages=site.regular_pages, size=1) | get_path(path=[\"total_pages\"]) }}|{{ paginate(pages=site.regular_pages, size=1) | get_path(path=[\"total_pages\"]) }}|{{ paginator() | get_path(path=[\"total_pages\"]) }}",
            &posts
        ),
        "5|5|5"
    );
    // The size defaults to `pagination.pagerSize`.
    let about = site.scope(site.page(PageKind::Page, "/about", 0), None);
    assert_eq!(
        site.render(
            "{{ paginate(pages=site.regular_pages) | get_path(path=[\"total_pages\"]) }}|{{ paginate(pages=site.regular_pages, size=2) | get_path(path=[\"total_pages\"]) }}",
            &about
        ),
        "3|3"
    );
}

#[test]
fn a_different_call_is_an_error_naming_both_positions() {
    let site = support::load();
    let posts = site.scope(site.page(PageKind::Section, "/posts", 0), None);
    let e = site
        .try_render(
            "{{ paginator() | get_path(path=[\"total_pages\"]) }}{{ partial(name=\"paginate_other\") }}",
            &posts,
        )
        .expect_err("conflicting pagination");
    let e = e.to_string();
    assert!(e.contains("already paginated"), "{e}");
    assert!(e.contains("layout of /posts (html)"), "first position: {e}");
    assert!(
        e.contains("_partials/paginate_other.html"),
        "second position: {e}"
    );
    // Grouped lists paginate too.
    let about = site.scope(site.page(PageKind::Page, "/about", 0), None);
    assert_eq!(
        site.render(
            "{% set g = site.regular_pages | group_by_param(param=\"series\") %}{% set p = paginate(pages=g, size=2) %}{{ p.total_pages }}:{% for x in p.pages %}{{ x.key }}={{ x.pages | length }};{% endfor %}",
            &about
        ),
        "2:x=2;"
    );
}

#[test]
fn wave_two_renders_pager_n_also_inside_partials() {
    let site = support::load();
    let home = site.page(PageKind::Home, "/", 0);
    assert_eq!(
        site.render(
            "{{ paginator() | get_path(path=[\"page_number\"]) }}",
            &site.scope(home, None)
        ),
        "1"
    );
    let s2 = site.scope(home, Some(2));
    assert_eq!(
        site.render(
            "{{ paginator() | get_path(path=[\"page_number\"]) }}|{{ partial(name=\"pager\") }}|{{ [p.title for p in paginator() | get_path(path=[\"pages\"])] | join(sep=\",\") }}",
            &s2
        ),
        "2|2/3|".to_owned()
            + &site.render(
                "{{ [p.title for p in paginator() | get_path(path=[\"pages\"])] | join(sep=\",\") }}",
                &s2
            )
    );
    let s3 = site.scope(home, Some(3));
    assert_eq!(site.render("{{ partial(name=\"pager\") }}", &s3), "3/3");
    assert_eq!(
        site.render("{{ paginator() | get_path(path=[\"has_next\"]) }}|{{ paginator() | get_path(path=[\"prev\", \"url\"]) }}", &s3),
        "false|/sub/page/2/"
    );
}

#[test]
fn pagination_needs_a_layout_scope() {
    let site = support::load();
    let mut s = site.home_scope();
    s.phase = Phase::Content;
    let e = site
        .try_render("{{ paginator() }}", &s)
        .expect_err("content phase");
    assert!(e.to_string().contains("only available in layouts"), "{e}");
    let e = site.render_bare("{{ paginator() }}").expect_err("no scope");
    assert!(e.to_string().contains("@__nh"), "{e}");
}
