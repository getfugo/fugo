//! The content engine on small sites: shortcode semantics, includes (placeholder renumbering
//! and `page_inner` spans), cycles, memo cells under two threads, buffered store writes, and
//! the JSON variant through a layout job.

use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use ssg_render::Job;
use ssg_view::{ContentRenderer, HookVariant, Phase, RenderScope};
use tera::{Kwargs, State, Value};

use crate::support::{content_scope, session_in, site, write};

const CONFIG: &str = "baseURL = \"https://example.org/\"\ntitle = \"Engine\"\ndisableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\", \"404\"]\n[security]\nenableInlineShortcodes = true\n[markup.goldmark.renderer]\nunsafe = true\n";

fn html(s: &crate::support::Site, path: &str) -> String {
    let id = s.page(path, 0);
    let scope = content_scope(&s.session, id, HookVariant::Html);
    s.session
        .content(id, HookVariant::Html, &scope)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .html
        .clone()
}

#[test]
fn shortcode_semantics() {
    let s = site(&[
        ("config.toml", CONFIG),
        (
            "content/sc.md",
            "---\ntitle: SC\n---\n{{< types 42 -7 3.5 true false \"42\" `raw` 0x10 1e3 >}}\n\n{{< types a=1 b=\"x\" c=true >}}\n\n{{< p >}} {{< p \"only\" >}} {{< p x=\"named\" >}}\n\n{{% outer %}}A {{< mid >}}B {{% leaf %}}*c*{{% /leaf %}}{{< /mid >}}{{% /outer %}}\n\n{{% md %}}**md**{{% /md %}} {{< md >}}**raw**{{< /md >}}\n\n{{</* esc \"x\" */>}} {{%/* esc */%}}\n\n  {{< multi >}}\n\n{{< greet.inline >}}Hi {{ page.title }} {{ shortcode.ordinal }}{{< /greet.inline >}} {{< greet.inline />}}\n",
        ),
        (
            "layouts/_shortcodes/types.html",
            "{{ shortcode.params | jsonify }}|{{ shortcode.args | length }}|{{ shortcode.is_named_params }}",
        ),
        (
            "layouts/_shortcodes/p.html",
            "[{{ shortcode | arg(index=0, default=\"dflt\") }}|{{ shortcode | arg(name=\"x\", default=\"no\") }}]",
        ),
        (
            "layouts/_shortcodes/outer.html",
            "<o{{ shortcode.ordinal }}>{{ inner }}</o>",
        ),
        (
            "layouts/_shortcodes/mid.html",
            "<m{{ shortcode.ordinal }} parent=\"{{ shortcode.parent.name }}{{ shortcode.parent.ordinal }}\">{{ inner }}</m>",
        ),
        (
            "layouts/_shortcodes/leaf.html",
            "<l{{ shortcode.ordinal }} gp=\"{{ shortcode.parent.parent.name }}\">{{ inner }}</l>",
        ),
        ("layouts/_shortcodes/md.html", "{{ inner }}"),
        ("layouts/_shortcodes/multi.html", "<div>\none\ntwo\n</div>"),
    ]);
    let out = html(&s, "/sc");
    // Parameter typing: bare numbers and booleans are typed, quoted and backtick strings, `0x10`
    // and `1e3` stay strings; named parameters are a map.
    assert!(
        out.contains(r#"[42,-7,3.5,true,false,"42","raw","0x10","1e3"]|9|false"#),
        "{out}"
    );
    assert!(out.contains(r#"{"a":1,"b":"x","c":true}|0|true"#), "{out}");
    // `arg`: defaults for missing positions and names, and for the other argument style.
    assert!(out.contains("[dflt|no] [only|no] [dflt|named]"), "{out}");
    // Nesting: ordinals per level, parents, a nested `{{% %}}` inner rendered as Markdown, the
    // outermost `{{% %}}` inner raw (then Markdown as part of the page).
    assert!(
        out.contains(r#"<o5>A <m0 parent="outer5">B <l0 gp="outer"><em>c</em></l></m></o>"#),
        "{out}"
    );
    // `%` output is Markdown, `<` output is not.
    assert!(out.contains("<strong>md</strong> **raw**"), "{out}");
    // Escaped shortcodes are text.
    assert!(
        out.contains("{{&lt; esc &ldquo;x&rdquo; &gt;}} {{% esc %}}"),
        "{out}"
    );
    // An indented call without inner content: its output lines get the indentation; the
    // paragraph that holds only its placeholder is removed.
    assert!(out.contains("<div>\n  one\n  two\n  </div>"), "{out}");
    // Inline shortcodes (enabled): the body is a Tera template, reused by a self-closed call.
    assert!(out.contains("<p>Hi SC 9 Hi SC 10</p>"), "{out}");
}

/// Page A includes B's source through `{{% include %}}`: B's placeholders are renumbered into
/// A's table, and hooks inside the included text see B as `page_inner`.
#[test]
fn includes_renumber_placeholders_and_set_page_inner() {
    let s = site(&[
        ("config.toml", CONFIG),
        (
            "content/a.md",
            "---\ntitle: A\n---\n{{< badge a1 >}} [own](x)\n\n{{% include \"/b\" %}}\n\n{{< badge a2 >}}\n",
        ),
        (
            "content/b.md",
            "---\ntitle: B\n---\n{{< badge b1 >}} [from b](y) {{< badge b2 >}}\n",
        ),
        (
            "layouts/_shortcodes/badge.html",
            "<b>{{ shortcode | arg(index=0) }}</b>",
        ),
        (
            "layouts/_shortcodes/include.html",
            "{{ render_shortcodes(page=get_page(path=shortcode | arg(index=0))) }}",
        ),
        (
            "layouts/_markup/render-link.html",
            "<a href=\"{{ destination }}\" data-inner=\"{{ page_inner.title }}\" data-page=\"{{ page.title }}\">{{ text }}</a>",
        ),
    ]);
    let out = html(&s, "/a");
    assert_eq!(
        out,
        "<p><b>a1</b> <a href=\"x\" data-inner=\"A\" data-page=\"A\">own</a></p>\n<p><b>b1</b> <a href=\"y\" data-inner=\"B\" data-page=\"A\">from b</a> <b>b2</b></p>\n<b>a2</b>\n"
    );
    // Outside the content phase, `render_shortcodes` is B's source with its outputs in place.
    let b = s.page("/b", 0);
    let a = s.page("/a", 0);
    let layout = RenderScope::layout(
        a,
        s.session.model().pages[a].lang,
        s.session.model().pages[a].formats[0],
        None,
    );
    let src = s
        .session
        .render_shortcodes(b, &layout)
        .expect("layout render_shortcodes");
    assert_eq!(src.markdown, "<b>b1</b> [from b](y) <b>b2</b>\n");
}

/// Go's handling of `{{% %}}` includes on a Markdown page: the included text sits between
/// context marker lines (Go's context `Wrap`) and an indented call indents every further line
/// of its output, the included text included. So an indented include stays in its definition,
/// an include ends the definition list before it, and a definition or tight list item that an
/// include ends keeps goldmark's newline before its end tag (expected: Go's goldmark
/// converter at 44529028 on the same Markdown).
#[test]
fn includes_are_wrapped_and_indented() {
    let s = site(&[
        ("config.toml", CONFIG),
        (
            "content/a.md",
            "---\ntitle: A\n---\nterm\n: Intro.\n  {{% include \"/b\" %}}\n\n  After.\n\nformat\n: Format.\n\n{{% include \"/c\" %}}\n",
        ),
        (
            "content/b.md",
            "---\ntitle: B\n---\n\nIncluded *text*.\n\n- one\n- two\n",
        ),
        ("content/c.md", "---\ntitle: C\n---\n\nparams\n: Params.\n"),
        (
            "layouts/_shortcodes/include.html",
            "{{ render_shortcodes(page=get_page(path=shortcode | arg(index=0))) }}",
        ),
    ]);
    assert_eq!(
        html(&s, "/a"),
        "<dl>\n<dt>term</dt>\n<dd>Intro.\n\n<p>Included <em>text</em>.</p>\n<ul>\n<li>one</li>\n<li>two\n</li>\n</ul>\n<p>After.</p>\n</dd>\n<dt>format</dt>\n<dd>Format.</dd>\n</dl>\n<dl>\n<dt>params</dt>\n<dd>Params.\n</dd>\n</dl>\n"
    );
}

/// The indentation of includes of a page with `{{< >}}` output, as Go does it: a `{{% %}}`
/// call (and a call nested in one) runs before Markdown, where the included page's `{{< >}}`
/// outputs are still placeholders, so only the included text is indented; a `{{< >}}` call
/// (and a call nested in one) runs after Markdown with those outputs in place, so they are
/// indented with it. Expected: the Go build at 44529028 on the same site (its templates in Go
/// syntax).
#[test]
fn includes_indent_like_go() {
    let s = site(&[
        ("config.toml", CONFIG),
        (
            "content/a.md",
            "---\ntitle: A\n---\n- item\n\n  {{< include \"/b\" >}}\n\n  {{% include \"/b\" %}}\n\n{{< outer >}}\n  {{< include \"/b\" >}}\n{{< /outer >}}\n\n{{% outer %}}\n  {{< include \"/b\" >}}\n{{% /outer %}}\n\n{{% outer %}}\n  {{% include \"/b\" %}}\n{{% /outer %}}\n",
        ),
        (
            "content/b.md",
            "---\ntitle: B\n---\nline1\n{{< multi >}}\nline2\n",
        ),
        (
            "layouts/_shortcodes/include.html",
            "{{ render_shortcodes(page=get_page(path=shortcode | arg(index=0))) }}\n",
        ),
        ("layouts/_shortcodes/multi.html", "<div>\nx\n</div>\n"),
        (
            "layouts/_shortcodes/outer.html",
            "<section>\n{{ inner }}</section>\n",
        ),
    ]);
    assert_eq!(
        html(&s, "/a"),
        "<ul>\n<li>\n<p>item</p>\nline1\n  <div>\n  x\n  </div>\n  \n  line2\n  \n\n<p>line1\n<div>\nx\n</div>\n\nline2</p>\n</li>\n</ul>\n<section>\n\n  line1\n  <div>\n  x\n  </div>\n  \n  line2\n  \n\n</section>\n\n<section>\n<p>line1\n<div>\nx\n</div>\n\nline2</p>\n</section>\n<section>\n<p>line1\n<div>\nx\n</div>\n\nline2</p>\n</section>\n"
    );
}

/// A content cycle through two pages is an error that names both files; memo cells never
/// block, so it does not hang.
#[test]
fn cycles_are_errors() {
    let s = site(&[
        ("config.toml", CONFIG),
        ("content/a.md", "---\ntitle: A\n---\n{{< show \"/b\" >}}\n"),
        ("content/b.md", "---\ntitle: B\n---\n{{< show \"/a\" >}}\n"),
        (
            "layouts/_shortcodes/show.html",
            "{{ page_content(page=get_page(path=shortcode | arg(index=0))) }}",
        ),
    ]);
    let err = s.session.render_content().expect_err("cycle").to_string();
    assert!(
        err.contains("is needed while it is being computed"),
        "{err}"
    );
    assert!(err.contains("a.md") && err.contains("b.md"), "{err}");
    // A page's own TOC from its shortcode is not a cycle (fragments without the calls).
    let s = site(&[
        ("config.toml", CONFIG),
        (
            "content/t.md",
            "---\ntitle: T\n---\n## One\n\n{{< toc >}}\n\n## Two\n",
        ),
        ("layouts/_shortcodes/toc.html", "{{ page_toc(page=page) }}"),
    ]);
    s.session.render_content().expect("own toc");
    assert!(html(&s, "/t").contains("<a href=\"#two\">Two</a>"));
}

/// Two threads inside memo computations at the same time (a barrier in a shortcode):
/// - A and B each need the other's fragments (the docs link-hook pattern): both finish;
/// - both threads compute A: both finish, one value wins, and A's buffered store write is
///   committed once.
#[test]
fn two_threads_never_deadlock_and_commit_once() {
    let files: Vec<(String, String)> = [
        ("config.toml", CONFIG),
        (
            "content/a.md",
            "---\ntitle: A\n---\n## In A\n\n{{< sync >}}{{< count >}}{{< toc \"/b\" >}}\n",
        ),
        (
            "content/b.md",
            "---\ntitle: B\n---\n## In B\n\n{{< sync >}}{{< toc \"/a\" >}}\n",
        ),
        ("layouts/_shortcodes/sync.html", "{{ sync() }}"),
        (
            "layouts/_shortcodes/count.html",
            "{{ store_set(key=\"n\", value=(store_get(key=\"n\") or 0) + 1) }}n={{ store_get(key=\"n\") }}",
        ),
        (
            "layouts/_shortcodes/toc.html",
            "{{ page_toc(page=get_page(path=shortcode | arg(index=0))) }}",
        ),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
    .collect();
    let run = |pages: [&'static str; 2]| {
        let dir = write(&files);
        let barrier = Arc::new(Barrier::new(2));
        let session = session_in(dir.path(), &|t: &mut tera::Tera, _| {
            let b = Arc::clone(&barrier);
            t.register_function("sync", move |_: Kwargs, _: &State| {
                b.wait();
                Ok::<_, tera::Error>(Value::from(""))
            });
        });
        let s = crate::support::Site { dir, session };
        let ids = pages.map(|p| s.page(p, 0));
        let (tx, rx) = mpsc::channel();
        for id in ids {
            let session = Arc::clone(&s.session);
            let tx = tx.clone();
            std::thread::spawn(move || {
                let scope = content_scope(&session, id, HookVariant::Html);
                let r = session.content(id, HookVariant::Html, &scope);
                let _ = tx.send(r.map_err(|e| e.to_string()));
            });
        }
        let results: Vec<_> = (0..2)
            .map(|_| {
                rx.recv_timeout(Duration::from_secs(60))
                    .expect("a memo computation blocked")
                    .expect("content")
            })
            .collect();
        (s, ids, results)
    };

    let (s, [a, b], results) = run(["/a", "/b"]);
    assert!(results.iter().all(|c| c.html.contains("TableOfContents")));
    let got_a = html(&s, "/a");
    assert!(got_a.contains("#in-b"), "{got_a}");
    assert!(html(&s, "/b").contains("#in-a"));
    let _ = b;

    let (s, _, results) = run(["/a", "/a"]);
    assert!(Arc::ptr_eq(&results[0], &results[1]), "one value wins");
    assert!(results[0].html.contains("n=1"));
    let n = s.session.page_stores().get(None, a, "n");
    assert_eq!(
        n,
        Some(Value::from(1)),
        "the winner's write is committed once"
    );
}

/// Store writes of a failing computation are dropped; layout-phase writes apply directly.
#[test]
fn store_writes_follow_the_winner() {
    let s = site(&[
        ("config.toml", CONFIG),
        ("content/w.md", "---\ntitle: W\n---\n{{< w >}}\n"),
        (
            "layouts/_shortcodes/w.html",
            "{{ store_set(key=\"k\", value=\"v\") }}{{ store_get(key=\"k\") }}",
        ),
    ]);
    let w = s.page("/w", 0);
    assert_eq!(s.session.page_stores().get(None, w, "k"), None);
    s.session.render_content().expect("content");
    assert!(html(&s, "/w").contains("v"));
    assert_eq!(
        s.session.page_stores().get(None, w, "k"),
        Some(Value::from("v"))
    );
    let mut layout = content_scope(&s.session, w, HookVariant::Html);
    layout.phase = Phase::Layout;
    s.session
        .page_stores()
        .set(layout.txn, w, "k", Value::from("layout"));
    assert_eq!(
        s.session.page_stores().get(None, w, "k"),
        Some(Value::from("layout"))
    );
}

/// A format with a render hook gets its own content variant; its layout job prints it.
#[test]
fn json_variant_through_a_layout_job() {
    let s = site(&[
        (
            "config.toml",
            "baseURL = \"https://example.org/\"\ndisableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\", \"404\", \"home\"]\n[outputs]\npage = [\"html\", \"json\"]\n",
        ),
        (
            "content/t.md",
            "---\ntitle: T\n---\n| a |\n|---|\n| *1* |\n",
        ),
        ("layouts/single.html", "{{ page.content }}"),
        ("layouts/single.json", "{{ page.content }}"),
        (
            "layouts/_markup/render-table.json.json",
            "{% for r in tbody %}{% for c in r %}{{ c.text | jsonify }}{% endfor %}{% endfor %}",
        ),
    ]);
    assert_eq!(s.session.variants().len(), 2);
    s.session.render_content().expect("content");
    s.session.freeze_views().expect("freeze");
    let t = s.page("/t", 0);
    let mut outs = Vec::new();
    for &format in &s.session.model().pages[t].formats {
        outs.extend(
            s.session
                .render_job(&Job::Page { page: t, format })
                .expect("job"),
        );
    }
    let texts: Vec<&str> = outs.iter().map(|o| o.text.as_str()).collect();
    assert!(texts[0].contains("<table>"), "{texts:?}");
    assert_eq!(
        texts[1], "\"\\u003cem\\u003e1\\u003c/em\\u003e\"",
        "{texts:?}"
    );
}

/// Phase C1 renders bundled content pages (resources) and HTML content; phase D freezes them
/// into the Full generation.
#[test]
fn c1_renders_bundled_pages_and_html_content() {
    let s = site(&[
        ("config.toml", CONFIG),
        ("content/b/index.md", "---\ntitle: B\n---\nOwner.\n"),
        (
            "content/b/sub.md",
            "---\ntitle: Sub\n---\nBundled {{< x >}}.\n",
        ),
        (
            "content/h.html",
            "---\ntitle: H\n---\n<div>{{< x >}}</div>\n<!--more-->\n<p>rest</p>\n",
        ),
        ("layouts/_shortcodes/x.html", "<i>x</i>"),
    ]);
    s.session.render_content().expect("content");
    s.session.freeze_views().expect("freeze");
    let full = s
        .session
        .views()
        .generation(Phase::Layout, HookVariant::Html);
    let content = |path: &str| {
        let v = full.full(s.page(path, 0));
        v.as_map()
            .and_then(|m| m.get(&tera::value::Key::Str("content")))
            .map(ToString::to_string)
            .unwrap_or_default()
    };
    assert_eq!(content("/b/sub.md"), "<p>Bundled <i>x</i>.</p>\n");
    let h = s.page("/h", 0);
    let c = s
        .session
        .content(
            h,
            HookVariant::Html,
            &content_scope(&s.session, h, HookVariant::Html),
        )
        .expect("html content");
    assert_eq!(c.summary, "<div><i>x</i></div>");
    assert_eq!(c.html, "<div><i>x</i></div>\n\n\n\n\n<p>rest</p>");
}

/// `.TableOfContents`: empty for HTML content files (Go computes a TOC only for converted
/// markup), `<nav id="TableOfContents"></nav>` for a markdown page without headings (Go's
/// goldmark TOC builder always writes the `nav`).
#[test]
fn toc_of_html_content_is_empty() {
    let s = site(&[
        ("config.toml", CONFIG),
        (
            "content/h.html",
            "---\ntitle: H\n---\n<h2 id=\"x\">X</h2>\n<p>Body</p>\n",
        ),
        ("content/m.md", "---\ntitle: M\n---\nNo headings.\n"),
    ]);
    let toc = |path: &str| {
        let id = s.page(path, 0);
        let scope = content_scope(&s.session, id, HookVariant::Html);
        s.session
            .content(id, HookVariant::Html, &scope)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .table_of_contents
            .clone()
    };
    assert_eq!(toc("/h"), "");
    assert_eq!(toc("/m"), "<nav id=\"TableOfContents\"></nav>");
}
