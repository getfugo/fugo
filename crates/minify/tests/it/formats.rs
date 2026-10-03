//! Behaviour of each minifier on small inputs.

use std::borrow::Cow;

use ssg_minify::options::{HtmlComments, TemplateSyntax, XmlComments, XmlWhitespace};
use ssg_minify::{JsonErrorKind, Minifier, MinifyError, MinifyTarget, Options, target_for};

fn min(target: MinifyTarget, input: &str) -> String {
    Minifier::default()
        .minify(target, input)
        .unwrap_or_else(|e| panic!("{e}: {input}"))
        .into_owned()
}

fn with(options: Options, target: MinifyTarget, input: &str) -> String {
    Minifier::with_options(options, &[])
        .minify(target, input)
        .unwrap_or_else(|e| panic!("{e}: {input}"))
        .into_owned()
}

#[test]
fn media_types() {
    use MinifyTarget::{Css, Html, Js, Json, Svg, Xml};
    let cases = [
        ("text/html", Some(Html)),
        ("text/html; charset=utf-8", Some(Html)),
        ("TEXT/HTML", Some(Html)),
        ("text/css", Some(Css)),
        ("text/javascript", Some(Js)),
        ("application/javascript", Some(Js)),
        ("application/x-javascript", Some(Js)),
        ("text/ecmascript", Some(Js)),
        ("application/json", Some(Json)),
        ("application/ld+json", Some(Json)),
        ("application/manifest+json", Some(Json)),
        ("text/x-json", Some(Json)),
        ("image/svg+xml", Some(Svg)),
        ("application/rss+xml", Some(Xml)),
        ("application/xml", Some(Xml)),
        ("application/atom+xml", Some(Xml)),
        ("text/xml", Some(Xml)),
        ("text/plain", None),
        ("application/geo+json", None),
        ("image/png", None),
        ("text/calendar", None),
        ("garbage", None),
    ];
    for (mt, want) in cases {
        assert_eq!(target_for(mt), want, "{mt}");
    }
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
    let html = "<style>a { color : red }</style>";
    assert_eq!(
        m.minify(MinifyTarget::Html, html).unwrap(),
        "<style>a { color : red }</style>"
    );
    assert_eq!(min(MinifyTarget::Html, html), "<style>a{color:red}</style>");
}

#[test]
fn html() {
    let page = "<!DOCTYPE html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\">\n    \
                <title>A &amp; B</title>\n  </head>\n  <body>\n    <!-- note -->\n    \
                <p class=\"x\">one\n   two</p>\n    <script>\n      var answer = 40 + 2;\n      \
                console.log(answer);\n    </script>\n  </body>\n</html>\n";
    assert_eq!(
        min(MinifyTarget::Html, page),
        "<!doctype html><html lang=en><head><meta charset=utf-8><title>A & B</title></head>\
         <body><p class=x>one two</p><script>console.log(42);</script></body></html>"
    );
    // Go's defaults keep end tags and the document tags.
    assert_eq!(
        min(
            MinifyTarget::Html,
            "<html><head></head><body><p>x</p></body></html>"
        ),
        "<html><head></head><body><p>x</p></body></html>"
    );
    let mut o = Options::default();
    o.html.keep_end_tags = false;
    o.html.keep_document_tags = false;
    assert_eq!(
        with(
            o,
            MinifyTarget::Html,
            "<html><head></head><body><p>x</p></body></html>"
        ),
        "<body><p>x"
    );
}

#[test]
fn html_comments() {
    let input = "<p>a <!-- c --> b <!--# include virtual=\"x\" --></p>";
    assert_eq!(
        min(MinifyTarget::Html, input),
        "<p>a b<!--# include virtual=\"x\" --></p>"
    );
    let mut o = Options::default();
    o.html.comments = HtmlComments::Remove;
    assert_eq!(with(o, MinifyTarget::Html, input), "<p>a b</p>");
    o.html.comments = HtmlComments::KeepAll;
    assert_eq!(
        with(o, MinifyTarget::Html, input),
        "<p>a <!-- c --> b<!--# include virtual=\"x\" --></p>"
    );
}

#[test]
fn html_templates() {
    let input = "<p title=\"{{ .Title }}\">  {{ if .x }}  a  {{ end }} </p>";
    let mut o = Options::default();
    o.html.templates = TemplateSyntax::Braces;
    let out = with(o, MinifyTarget::Html, input);
    assert!(
        out.contains("{{ if .x }}") && out.contains("{{ end }}"),
        "{out}"
    );
}

#[test]
fn css() {
    assert_eq!(
        min(
            MinifyTarget::Css,
            "/* c */\na {\n  color: #ff0000;\n  margin: 0px 0px 0px 0px;\n}\n\n.b { color: rgba(0, 0, 0, 0.5) }\n"
        ),
        "a{color:red;margin:0}.b{color:rgba(0,0,0,.5)}"
    );
    let mut o = Options::default();
    o.css.keep_css2 = false;
    assert_eq!(
        with(o, MinifyTarget::Css, ".b { color: rgba(0, 0, 0, 0.5) }"),
        ".b{color:#00000080}"
    );
    let err = Minifier::default()
        .minify(MinifyTarget::Css, "a { color: red; ")
        .map(|_| ());
    assert!(err.is_ok(), "an unclosed block at the end is valid CSS");
    // Invalid CSS passes through (css_tolerance.rs).
    assert_eq!(min(MinifyTarget::Css, "a { b: ) }"), "a{b:)}");
}

#[test]
fn js() {
    let gtag = "window.dataLayer = window.dataLayer || [];\n function gtag(){dataLayer.push(arguments);}\n \
                gtag('js', new Date());\n\n gtag('config', 'UA-000000000-1');";
    assert_eq!(
        min(MinifyTarget::Js, gtag),
        "window.dataLayer=window.dataLayer||[];function gtag(){dataLayer.push(arguments)}\
         gtag(`js`,new Date),gtag(`config`,`UA-000000000-1`);"
    );
    let local =
        "function f(input){ let longName = input * 2; return longName + g(longName); }\nf(1);";
    assert_eq!(
        min(MinifyTarget::Js, local),
        "function f(e){let t=e*2;return t+g(t)}f(1);"
    );
    let mut o = Options::default();
    o.js.keep_var_names = true;
    assert_eq!(
        with(o, MinifyTarget::Js, local),
        "function f(input){let longName=input*2;return longName+g(longName)}f(1);"
    );
    // Modules are detected.
    assert_eq!(
        min(
            MinifyTarget::Js,
            "import { a } from './a.js';\nexport const b = a + 1;\n"
        ),
        "import{a}from\"./a.js\";export const b=a+1;"
    );
    let err = Minifier::default().minify(MinifyTarget::Js, "var = ;");
    assert!(matches!(err, Err(MinifyError::Js(_))), "{err:?}");
}

#[test]
fn json() {
    assert_eq!(
        min(
            MinifyTarget::Json,
            "{\n  \"z\": [1, 2.50, -0.5e+10, true, false, null],\n  \"a\": { \"s\": \"x \\\" \\u00e9 y\" },\n  \"e\": {}\n}\n"
        ),
        "{\"z\":[1,2.50,-0.5e+10,true,false,null],\"a\":{\"s\":\"x \\\" \\u00e9 y\"},\"e\":{}}"
    );
    // Go's leniencies: trailing commas are dropped, raw control characters kept.
    assert_eq!(
        min(MinifyTarget::Json, "{ \"a\": [1, 2, ], \"b\": \"x\n y\", }"),
        "{\"a\":[1,2],\"b\":\"x\n y\"}"
    );
    assert_eq!(min(MinifyTarget::Json, " \n "), "");
    assert_eq!(min(MinifyTarget::Json, " 12 "), "12");
    let cases = [
        ("{\"a\" 1}", 5, JsonErrorKind::Unexpected('1')),
        ("[1,,2]", 3, JsonErrorKind::Unexpected(',')),
        ("[01]", 2, JsonErrorKind::Unexpected('1')),
        ("[1.]", 3, JsonErrorKind::Number),
        ("[-]", 2, JsonErrorKind::Number),
        ("\"\\x\"", 1, JsonErrorKind::Escape),
        ("{\"a\":1", 6, JsonErrorKind::Eof),
        ("1 2", 2, JsonErrorKind::Unexpected('2')),
        ("[,]", 1, JsonErrorKind::Unexpected(',')),
        ("{,}", 1, JsonErrorKind::Unexpected(',')),
        ("nul", 0, JsonErrorKind::Unexpected('n')),
    ];
    for (input, offset, kind) in cases {
        match Minifier::default().minify(MinifyTarget::Json, input) {
            Err(MinifyError::Json { offset: o, kind: k }) => {
                assert_eq!((o, k), (offset, kind), "{input}");
            }
            other => panic!("{input}: {other:?}"),
        }
    }
}

#[test]
fn xml() {
    let rss = "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\" ?>\n\
               <rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n  \
               <!-- feed -->\n  <channel>\n    <title>A &amp; B</title>\n    \
               <atom:link href=\"https://x.org/index.xml\" rel=\"self\" type=\"application/rss+xml\" />\n    \
               <description><![CDATA[<p>x</p>]]></description>\n    <item>\n      \
               <title>\n        Two\n        words   here\n      </title>\n      <guid></guid>\n      \
               <category> </category>\n    </item>\n  </channel>\n</rss>\n";
    assert_eq!(
        min(MinifyTarget::Xml, rss),
        "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\
         <rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\"><channel>\
         <title>A &amp; B</title>\
         <atom:link href=\"https://x.org/index.xml\" rel=\"self\" type=\"application/rss+xml\"/>\
         <description><![CDATA[<p>x</p>]]></description><item><title>Two\nwords here</title>\
         <guid/><category/></item></channel></rss>"
    );
    let mut o = Options::default();
    o.xml.whitespace = XmlWhitespace::Keep;
    assert_eq!(
        with(
            o,
            MinifyTarget::Xml,
            "<a>\n  <b x='\"q\"' >  t  </b >\n</a>"
        ),
        "<a>\n  <b x='\"q\"'>  t  </b>\n</a>"
    );
    assert_eq!(
        min(
            MinifyTarget::Xml,
            "<!DOCTYPE  note SYSTEM \"n.dtd\" ><note><?pi  x ?></note>"
        ),
        "<!DOCTYPE note SYSTEM \"n.dtd\"><note><?pi  x?></note>"
    );
    let err = Minifier::default().minify(MinifyTarget::Xml, "<a></b>");
    assert!(matches!(err, Err(MinifyError::Xml { .. })), "{err:?}");
}

#[test]
fn svg() {
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\">\n  <!-- icon -->\n  \
               <path d=\"M0 0L10 10\" />\n  <text x=\"1\"> Hi  there </text>\n</svg>\n";
    assert_eq!(
        min(MinifyTarget::Svg, svg),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\">\
         <path d=\"M0 0L10 10\"/><text x=\"1\">Hi there</text></svg>"
    );
    let mut o = Options::default();
    o.svg.comments = XmlComments::Keep;
    assert!(with(o, MinifyTarget::Svg, svg).contains("<!-- icon -->"));
}
