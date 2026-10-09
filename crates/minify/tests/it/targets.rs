//! CSS with browser targets from a browserslist configuration: prefixes added and syntax lowered
//! for the targets, hand-written fallbacks kept as written; CSS in HTML (`<style>`, `style`)
//! printed for the targets.

use ssg_minify::{Minifier, MinifyTarget, project_browsers};

/// A minifier with the targets of `queries` (a `.browserslistrc`).
fn minifier(queries: &str) -> Minifier {
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(dir.path().join(".browserslistrc"), queries).expect("write");
    let browsers = project_browsers(dir.path(), "production").expect("browserslist");
    assert!(browsers.is_some(), "no targets for {queries:?}");
    Minifier::default().with_browsers(browsers)
}

/// Minifies `input` and checks that the result is a fixed point.
fn min(m: &Minifier, input: &str) -> String {
    min_as(m, MinifyTarget::Css, input)
}

fn min_as(m: &Minifier, target: MinifyTarget, input: &str) -> String {
    let out = m
        .minify(target, input)
        .unwrap_or_else(|e| panic!("{e}: {input}"))
        .into_owned();
    let again = m.minify(target, &out).expect("again").into_owned();
    assert_eq!(again, out, "not idempotent for {input:?}");
    out
}

#[test]
fn prefixes_for_the_targets() {
    let input = ".a{user-select:none;backdrop-filter:blur(2px)}";
    let out = min(&minifier("Safari >= 12\nFirefox >= 60\n"), input);
    for want in [
        "-webkit-user-select:none",
        "-moz-user-select:none",
        "user-select:none",
        "-webkit-backdrop-filter:blur(2px)",
        "backdrop-filter:blur(2px)",
    ] {
        assert!(out.contains(want), "{want} missing: {out}");
    }
    // Prefixes none of the targets needs go.
    let out = min(
        &minifier("Chrome >= 120\n"),
        ".r{-webkit-border-radius:2px;border-radius:2px}",
    );
    assert_eq!(out, ".r{border-radius:2px}");
    // Without targets nothing is added.
    let out = min(&Minifier::default(), input);
    assert_eq!(out, input);
}

#[test]
fn syntax_lowered_for_the_targets() {
    let out = min(&minifier("Safari >= 12\n"), ".o{inset:0}");
    assert!(
        out.contains("top:0") && out.contains("left:0") && !out.contains("inset"),
        "{out}"
    );
}

/// A rule declaring a property twice is kept as written, in a group rule too; the rules around it
/// still get their prefixes.
#[test]
fn fallback_rules_kept_as_written() {
    let m = minifier("Safari >= 12\n");
    let gradient = "background:radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%);\
                    background:-webkit-radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%)";
    for input in [
        format!(".i{{{gradient}}}.a{{user-select:none}}"),
        format!("@media (min-width:1px){{.i{{{gradient}}}}}.a{{user-select:none}}"),
    ] {
        let out = min(&m, &input);
        assert!(
            out.contains("background:radial-gradient(")
                && out.contains("background:-webkit-radial-gradient("),
            "fallback lost: {out}"
        );
        assert!(out.contains("-webkit-user-select:none"), "{out}");
    }
}

/// A page's `<style>` is printed for the targets: no range syntax in media queries, no
/// `#rrggbbaa` (minify-html's own CSS minification has no targets). Nothing is added or merged.
#[test]
fn style_elements_for_the_targets() {
    let m = minifier("Safari >= 12\n");
    let html = |s: &str| min_as(&m, MinifyTarget::Html, s);
    let css = "@media (min-width: 576px) { .a { color: rgba(0, 0, 0, .5); user-select: none } }";
    let printed = "@media (min-width:576px){.a{color:rgba(0,0,0,.5);user-select:none}}";
    assert_eq!(
        html(&format!("<style>\n  {css}\n</style>")),
        format!("<style>{printed}</style>")
    );
    assert_eq!(
        html(&format!("<noscript><style>{css}</style></noscript>")),
        format!("<noscript><style>{printed}</style></noscript>")
    );
    assert_eq!(
        html(&format!("<svg><style>{css}</style></svg>")),
        format!("<svg><style>{printed}</style></svg>")
    );
    assert_eq!(
        html("<style>.i { display: -webkit-box; display: flex }</style>"),
        "<style>.i{display:-webkit-box;display:flex}</style>"
    );
    // CSS a pipeline wrote for the targets stays as it is: lowering it again would add its
    // prefixed rules once more.
    let sheet = min(
        &m,
        ".f::file-selector-button{margin:0}.a{user-select:none;color:rgba(0,0,0,.5)}",
    );
    assert!(sheet.contains("::-webkit-file-upload-button"), "{sheet}");
    let page = format!("<style>{sheet}</style>");
    assert_eq!(html(&page), page);
    // SVG markup in a `<style>` (a reference, CDATA) is kept as written.
    for page in [
        "<svg><style>.a &gt; .b { fill: red }</style></svg>",
        "<svg><style><![CDATA[ .a { fill: red } ]]></style></svg>",
    ] {
        assert_eq!(html(page), page);
    }
}

/// `style` attributes too, then written as minify-html writes attributes (unquoted when it can,
/// after the quoted ones).
#[test]
fn style_attributes_for_the_targets() {
    let m = minifier("Safari >= 12\n");
    let html = |s: &str| min_as(&m, MinifyTarget::Html, s);
    for (input, want) in [
        (
            "<p style=\"top: 0; right: 0; bottom: 0; left: 0; color: rgba(0,0,0,.5)\">x</p>",
            "<p style=top:0;right:0;bottom:0;left:0;color:rgba(0,0,0,.5)>x</p>",
        ),
        (
            "<p style=\"user-select : none\" title=\"a b\">x</p>",
            "<p title=\"a b\" style=user-select:none>x</p>",
        ),
        (
            "<p style=\"font-family: 'A B', &quot;C&quot;; content: 'a  b'\">x</p>",
            "<p style='font-family:A B,C;content:\"a  b\"'>x</p>",
        ),
        (
            "<p style=\"display: -webkit-box; display: flex\">x</p>",
            "<p style=display:-webkit-box;display:flex>x</p>",
        ),
        (
            "<svg><path d=\"M0 0\" style=\"fill: rgba(0,0,0,.5)\"/></svg>",
            "<svg><path d=\"M0 0\" style=fill:rgba(0,0,0,.5) /></svg>",
        ),
        // Kept as minify-html writes them: CSS that does not parse, a `}`, CDATA.
        (
            "<p style=\"color: red }\">x</p>",
            "<p style=\"color: red }\">x</p>",
        ),
        (
            "<p style=\"color: red} p {color: blue\">x</p>",
            "<p style=\"color: red} p {color: blue\">x</p>",
        ),
        (
            "<svg><![CDATA[ <a style=\"x : y\"> ]]></svg>",
            "<svg><![CDATA[ <a style=\"x : y\">]]></svg>",
        ),
    ] {
        assert_eq!(html(input), want);
    }
}

/// Without targets, inline CSS is printed compactly too.
#[test]
fn inline_css_without_targets() {
    let m = Minifier::default();
    assert_eq!(
        min_as(
            &m,
            MinifyTarget::Html,
            "<style>.o { top: 0; right: 0; bottom: 0; left: 0 }</style>\
             <p style=\"top: 0; right: 0; bottom: 0; left: 0\">x</p>"
        ),
        "<style>.o{top:0;right:0;bottom:0;left:0}</style><p style=top:0;right:0;bottom:0;left:0>x</p>"
    );
}
