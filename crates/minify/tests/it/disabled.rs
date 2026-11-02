//! Disabled types (`disableCSS`, …): passed through untouched, also where a page embeds them.

use std::borrow::Cow;

use ssg_minify::{Minifier, MinifyTarget, Options};

fn html(disabled: &[MinifyTarget], options: Options, input: &str) -> String {
    Minifier::with_options(options, disabled)
        .minify(MinifyTarget::Html, input)
        .unwrap_or_else(|e| panic!("{e}: {input}"))
        .into_owned()
}

#[test]
fn disabled_and_unknown_types_pass_through() {
    let m = Minifier::with_options(Options::default(), &[MinifyTarget::Css]);
    let css = "a { color : red }";
    assert!(matches!(m.minify(MinifyTarget::Css, css), Ok(Cow::Borrowed(s)) if s == css));
    assert!(!m.is_enabled(MinifyTarget::Css));
    assert!(matches!(
        m.minify_media_type("text/plain", " x "),
        Ok(Cow::Borrowed(" x "))
    ));
    assert_eq!(
        m.minify_media_type("application/json", "{ \"a\" : 1 }")
            .unwrap(),
        "{\"a\":1}"
    );
    // HTML leaves inline CSS alone when CSS is disabled.
    let page = "<style>a { color : red }</style>";
    assert_eq!(
        m.minify(MinifyTarget::Html, page).unwrap(),
        "<style>a { color : red }</style>"
    );
    assert_eq!(
        html(&[], Options::default(), page),
        "<style>a{color:red}</style>"
    );
    // And inline JavaScript and JSON when those are disabled.
    let m = Minifier::with_options(Options::default(), &[MinifyTarget::Js, MinifyTarget::Json]);
    let page = "<script>f( 1 )</script><script type=application/ld+json>{ \"a\" : 1 }</script>";
    assert_eq!(m.minify(MinifyTarget::Html, page).unwrap(), page);
}

#[test]
fn inline_svg_is_kept_as_written() {
    use MinifyTarget::Svg;
    let svg = "<svg width=\"10\"  height=\"10\">\n  <!-- c -->\n  \
               <style> rect { fill : red } </style>\n  <rect  style=\"fill : red\" width=\"10\"/>\n</svg>";
    let page = format!("<p>a  {svg}  b</p>");
    assert_eq!(
        html(&[], Options::default(), &page),
        "<p>a <svg height=10 width=10><style>rect{fill:red}</style><rect style=fill:red width=10 /></svg> b</p>"
    );
    // The page around it is minified as before; the element, its comments, its `<style>` and
    // its `style` attributes are not.
    assert_eq!(
        html(&[Svg], Options::default(), &page),
        format!("<p>a {svg} b</p>")
    );
    let page = format!("<div>\n  {svg}\n</div>\n<p>b</p>");
    assert_eq!(
        html(&[Svg], Options::default(), &page),
        format!("<div>{svg}</div><p>b</p>")
    );
    // Without optional end tags, and with nested, several and self-closing elements.
    let mut o = Options::default();
    o.html.keep_end_tags = false;
    let page = "<ul><li><SVG> <svg> <g/> </svg> </SVG></li><li><svg  /></li></ul>";
    assert_eq!(
        html(&[Svg], o, page),
        "<ul><li><SVG> <svg> <g/> </svg> </SVG><li><svg  /></ul>"
    );
    // A page without SVG is minified as when SVG is enabled.
    let page = "<p>a  <b>b</b>  c</p>";
    assert_eq!(
        html(&[Svg], Options::default(), page),
        "<p>a <b>b</b> c</p>"
    );
}
