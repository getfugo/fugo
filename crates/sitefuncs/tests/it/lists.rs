//! Page orders and groupings; taxonomy term orders.

use ssg_base::PageKind;

use crate::support;

fn titles(filter: &str) -> String {
    format!(
        "{{{{ [p.title for p in get_page(path=\"/posts\") | get_path(path=[\"regular_pages\"]) | {filter}] | join(sep=\",\") }}}}"
    )
}

#[test]
fn page_orders() {
    let site = support::load();
    let s = site.home_scope();
    assert_eq!(site.render(&titles("by_title"), &s), "Bundle,One,Three,Two");
    // link titles: Bundle, One, Three, Zwei.
    assert_eq!(
        site.render(&titles("by_link_title"), &s),
        "Bundle,One,Three,Two"
    );
    assert_eq!(site.render(&titles("by_date"), &s), "Bundle,One,Two,Three");
    assert_eq!(
        site.render(&titles("by_date | reverse"), &s),
        "Three,Two,One,Bundle"
    );
    assert_eq!(
        site.render(&titles("by_lastmod"), &s),
        "Bundle,Two,One,Three"
    );
    // Go's default page order: weight (0 last), then date descending.
    assert_eq!(
        site.render(&titles("by_weight"), &s),
        "Two,One,Three,Bundle"
    );
    let e = site
        .try_render("{{ [1, 2] | by_title }}", &s)
        .expect_err("not pages");
    assert!(e.to_string().contains("expected a page"), "{e}");
}

#[test]
fn groupings() {
    let site = support::load();
    let s = site.home_scope();
    assert_eq!(
        site.render(
            "{% for g in get_page(path=\"/posts\") | get_path(path=[\"regular_pages\"]) | group_by_date(format=\"%Y\") %}{{ g.key }}:{{ [p.title for p in g.pages] | join(sep=\",\") }};{% endfor %}",
            &s
        ),
        "2022:Three,Two;2021:One;2020:Bundle;"
    );
    assert_eq!(
        site.render(
            "{% for g in get_page(path=\"/posts\") | get_path(path=[\"regular_pages\"]) | group_by_date(format=\"%Y\", attribute=\"lastmod\") %}{{ g.key }};{% endfor %}",
            &s
        ),
        "2023;2021;2020;"
    );
    assert_eq!(
        site.render(
            "{% for g in get_page(path=\"/posts\") | get_path(path=[\"regular_pages\"]) | by_weight | group_by_param(param=\"series\") %}{{ g.key }}:{{ [p.title for p in g.pages] | join(sep=\",\") }};{% endfor %}",
            &s
        ),
        "x:One,Three;z:Two;"
    );
}

#[test]
fn taxonomy_term_orders() {
    let site = support::load();
    let s = site.scope(site.page(PageKind::Home, "/", 0), None);
    // a: one, two; b: one, three.
    assert_eq!(
        site.render(
            "{% for t in site.taxonomies.tags | by_count %}{{ t.name }}={{ t.count }};{% endfor %}",
            &s
        ),
        "a=2;b=2;"
    );
    assert_eq!(
        site.render(
            "{{ [t.name for t in site.taxonomies.tags | alphabetical] | join(sep=\",\") }}",
            &s
        ),
        "a,b"
    );
}

/// Pages for the Go tie rules: equal dates, zero dates, terms whose collation order differs
/// from Go's byte order.
const TIES: &[(&str, &str)] = &[
    (
        "content/notes/a.md",
        "---\ntitle: A\ndate: 2021-03-01T00:00:00Z\ntags: [\"x&y\", \"x.y\", \"éclair\", \"fudge\"]\n---\n",
    ),
    (
        "content/notes/b.md",
        "---\ntitle: B\ndate: 2021-03-01T00:00:00Z\n---\n",
    ),
    (
        "content/notes/c.md",
        "---\ntitle: C\ndate: 2021-01-01T00:00:00Z\n---\n",
    ),
    ("content/notes/d.md", "---\ntitle: D\n---\n"),
    ("content/notes/e.md", "---\ntitle: E\n---\n"),
];

/// Go's `GroupByDate`: a stable sort by date ascending, then the whole list reversed (so
/// pages with the same date come in reverse input order); the zero date is formatted too
/// (`"0001"` for `"2006"`).
#[test]
fn group_by_date_matches_go_ties_and_zero_dates() {
    let site = support::load_with(TIES);
    let s = site.home_scope();
    let groups = |kw: &str| {
        site.render(
            &format!(
                "{{% for g in get_page(path=\"/notes\") | get_path(path=[\"regular_pages\"]) | by_title | group_by_date({kw}) %}}{{{{ g.key }}}}:{{{{ [p.title for p in g.pages] | join(sep=\",\") }}}};{{% endfor %}}"
            ),
            &s,
        )
    };
    assert_eq!(groups("format=\"%Y\""), "2021:B,A,C;0001:E,D;");
    assert_eq!(
        groups("format=\"%Y-%m\""),
        "2021-03:B,A;2021-01:C;0001-01:E,D;"
    );
    // Go's `asc` (and its `rev`/`reverse` synonyms) keeps the ascending sort.
    assert_eq!(
        groups("format=\"%Y\", order=\"asc\""),
        "0001:D,E;2021:C,A,B;"
    );
}

/// Go's `ByCount`: count descending, then the lower-cased term by Go's `compare.Strings`
/// (case-folded code points, not the language's collation).
#[test]
fn by_count_ties_use_go_string_order() {
    let site = support::load_with(TIES);
    let s = site.scope(site.page(PageKind::Home, "/", 0), None);
    assert_eq!(
        site.render(
            "{% for t in site.taxonomies.tags | by_count %}{{ t.name | safe }}={{ t.count }};{% endfor %}",
            &s
        ),
        "a=2;b=2;fudge=1;x&y=1;x.y=1;éclair=1;"
    );
}
