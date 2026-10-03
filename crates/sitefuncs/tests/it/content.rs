//! Content through the render session, `markdownify`, `render_string`, `highlight`.

use ssg_base::PageKind;
use ssg_view::{Phase, Stage};

use crate::support;

#[test]
fn page_content_and_friends() {
    let site = support::load();
    let home = site.home_scope();
    let q = |f: &str| format!("{{{{ {f}(page=get_page(path=\"/posts/one\")) }}}}");
    assert_eq!(
        site.render(&q("page_content"), &home),
        "<p>content of One</p>"
    );
    assert_eq!(site.render(&q("page_summary"), &home), "summary of One");
    assert_eq!(site.render(&q("page_plain"), &home), "plain One");
    assert_eq!(site.render(&q("page_word_count"), &home), "3");
    assert_eq!(site.render(&q("page_toc"), &home), "<nav></nav>");
    assert_eq!(
        site.render(
            "{{ page_fragments(page=get_page(path=\"/posts/one\")) | get_path(path=[\"identifiers\"]) | join(sep=\",\") }}",
            &home
        ),
        "hello"
    );
    assert_eq!(
        site.render(&q("render_shortcodes"), &home),
        "before <b>zero</b> after <i>one</i>"
    );
    // Without a scope, `page=` is enough.
    assert_eq!(
        site.render_bare("{{ page_plain(page=get_page(path=\"/posts/two\")) }}")
            .expect("bare"),
        "plain Two"
    );
}

#[test]
fn the_callers_chain_reaches_the_session() {
    let site = support::load();
    let one = site.page(PageKind::Page, "/posts/one", 0);
    let mut s = site.home_scope();
    s.phase = Phase::Content;
    s.chain = vec![(one, Stage::Content(s.variant))];
    let e = site
        .try_render("{{ page_content(page=get_page(path=\"/posts/one\")) }}", &s)
        .expect_err("cycle");
    let e = e.to_string();
    assert!(
        e.contains("page_content(page=/posts/one)") && e.contains("cycle"),
        "{e}"
    );
}

#[test]
fn markdownify_and_render_string() {
    let site = support::load();
    let one = site.scope(site.page(PageKind::Page, "/posts/one", 0), None);
    assert_eq!(
        site.render("{{ \"*a*\" | markdownify }}", &one),
        "*a*<!-- One depth 1 -->"
    );
    assert_eq!(
        site.render("{{ \"*a*\" | render_string(display=\"block\") }}", &one),
        "<p>*a*</p><!-- One depth 1 -->"
    );
    assert_eq!(
        site.render(
            "{{ \"x\" | render_string(page=get_page(path=\"/about\")) }}",
            &one
        ),
        "x<!-- About depth 1 -->"
    );
    let e = site
        .try_render("{{ \"x\" | render_string(display=\"wide\") }}", &one)
        .expect_err("bad display");
    assert!(
        e.to_string().contains("expected \"inline\" or \"block\""),
        "{e}"
    );
    let e = site
        .render_bare("{{ \"x\" | markdownify }}")
        .expect_err("no scope");
    assert!(e.to_string().contains("@__nh"), "{e}");
}

#[test]
fn highlight_uses_the_site_defaults_and_options() {
    let site = support::load();
    let s = site.home_scope();
    let out = site.render("{{ \"let x = 1;\" | highlight(lang=\"rust\") }}", &s);
    assert!(out.starts_with("<div class=\"highlight\"><pre"), "{out}");
    assert!(
        out.contains("<span style=\"color:#66d9ef\">let</span>"),
        "{out}"
    );
    assert!(!out.contains("&lt;span"), "safe output: {out}");
    let inline = site.render(
        "{{ \"let x = 1;\" | highlight(lang=\"rust\", options={\"hl_inline\": true}) }}",
        &s,
    );
    assert!(inline.starts_with("<code"), "{inline}");
    let lines = site.render(
        "{{ \"a\nb\" | highlight(lang=\"text\", options=\"linenos=table\") }}",
        &s,
    );
    assert!(lines.contains("<table"), "{lines}");
    let e = site
        .try_render(
            "{{ \"x\" | highlight(lang=\"rust\", options=\"linenos\") }}",
            &s,
        )
        .expect_err("bad option string");
    assert!(
        e.to_string().starts_with("highlight") || e.to_string().contains("highlight"),
        "{e}"
    );
}
