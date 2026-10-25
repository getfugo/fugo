//! `related`: an index per candidate list, keyword lists, heading ids (the test session gives
//! every page the heading id `hello`), and searches without a page.

use ssg_base::PageKind;

use crate::support::{self, Site};

/// The titles of the pages `call` returns in the layout of `page`, in order.
fn titles(site: &Site, page: &str, call: &str) -> String {
    let s = site.scope(site.page(PageKind::Page, page, 0), None);
    site.render(
        &format!("{{{{ [q.title for q in {call}] | join(sep=\",\") }}}}"),
        &s,
    )
}

/// The test site with a fragments index besides `tags`.
fn with_fragments() -> Site {
    let config = format!(
        "{}[[related.indices]]\nname = \"fragmentrefs\"\ntype = \"fragments\"\napplyFilter = true\nweight = 50\n",
        support::FILES[0].1
    );
    support::load_with(&[("config.toml", &config)])
}

#[test]
fn related_uses_an_index_per_candidate_list() {
    let site = support::load();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    // one has tags a, b: two (a) and three (b) match, the newer first; one itself never does.
    let all = "related(pages=site.regular_pages)";
    assert_eq!(titles(&site, "/posts/one", all), "Three,Two");
    // Again: the list is known by its allocation.
    assert_eq!(titles(&site, "/posts/one", all), "Three,Two");
    assert_eq!(
        site.render(
            "{{ related(pages=site.regular_pages, limit=1) | length }}",
            &one
        ),
        "1"
    );
    assert_eq!(
        titles(
            &site,
            "/posts/one",
            "related(pages=[get_page(path=\"/posts/two\")])"
        ),
        "Two"
    );
    // Index names as the configuration has them, in lower case.
    assert_eq!(
        titles(
            &site,
            "/posts/one",
            "related(pages=site.regular_pages, indices=[\"TAGS\"])"
        ),
        "Three,Two"
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
fn keywords_add_to_the_page_or_search_alone() {
    let site = support::load();
    // three has tag b (one); `a` adds one and two, and one matches twice.
    assert_eq!(
        titles(
            &site,
            "/posts/three",
            "related(pages=site.regular_pages, keywords={\"tags\": [\"a\"]})"
        ),
        "One,Two"
    );
    // Without the page, the keywords alone: one value stands for a list of one, and the index
    // compares in lower case (`toLower`).
    assert_eq!(
        titles(
            &site,
            "/posts/three",
            "related(pages=site.regular_pages, page=none, keywords={\"Tags\": \"A\"})"
        ),
        "Two,One"
    );
    // One shares both values, two and three one each (the newer first).
    assert_eq!(
        titles(
            &site,
            "/posts/three",
            "related(pages=site.regular_pages, page=none, keywords={\"tags\": [\"a\", \"b\"]})"
        ),
        "One,Three,Two"
    );
    let three = site.scope(site.page(PageKind::Page, "/posts/three", 0), None);
    let e = site
        .try_render(
            "{{ related(pages=site.regular_pages, keywords={\"nope\": [\"a\"]}) }}",
            &three,
        )
        .expect_err("unknown index");
    assert!(e.to_string().contains("nope"), "{e}");
    let e = site
        .try_render(
            "{{ related(pages=site.regular_pages, keywords=[\"a\"]) }}",
            &three,
        )
        .expect_err("not a map");
    assert!(e.to_string().contains("expected a map"), "{e}");
}

#[test]
fn fragments_indices_read_the_headings_of_the_session() {
    let site = with_fragments();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    let n: usize = site
        .render("{{ site.regular_pages | length }}", &one)
        .parse()
        .expect("count");
    // Every page has heading `hello`: every other page is related to one.
    assert_eq!(
        site.render("{{ related(pages=site.regular_pages) | length }}", &one),
        (n - 1).to_string()
    );
    // The matched heading ids replace each result's fragments (`applyFilter`).
    assert_eq!(
        site.render(
            "{% for q in related(pages=site.regular_pages, limit=2) %}{{ q.fragments.identifiers | join(sep=\"+\") }};{% endfor %}",
            &one
        ),
        "hello;hello;"
    );
    // Heading ids alone.
    assert_eq!(
        site.render(
            "{{ related(pages=site.regular_pages, page=none, fragments=[\"hello\"]) | length }}",
            &one
        ),
        n.to_string()
    );
    assert_eq!(
        site.render(
            "{{ related(pages=site.regular_pages, page=none, fragments=[\"nope\"]) | length }}",
            &one
        ),
        "0"
    );
    // Without a fragments index, heading ids find nothing.
    let plain = support::load();
    let one = plain.scope(plain.page(PageKind::Page, "/posts/one", 0), None);
    assert_eq!(
        plain.render(
            "{{ related(pages=site.regular_pages, page=none, fragments=[\"hello\"]) | length }}",
            &one
        ),
        "0"
    );
}
