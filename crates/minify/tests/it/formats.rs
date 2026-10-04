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
    // And inline JavaScript and JSON when those are disabled.
    let m = Minifier::with_options(Options::default(), &[MinifyTarget::Js, MinifyTarget::Json]);
    let html = "<script>f( 1 )</script><script type=application/ld+json>{ \"a\" : 1 }</script>";
    assert_eq!(m.minify(MinifyTarget::Html, html).unwrap(), html);
}

#[test]
fn html() {
    let page = "<!DOCTYPE html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\">\n    \
                <title>A &amp; B</title>\n  </head>\n  <body>\n    <!-- note -->\n    \
                <p class=\"x\">one\n   two</p>\n    <script>\n      var answer = 40 + 2;\n      \
                console.log(answer);\n    </script>\n  </body>\n</html>\n";
    // `answer` is a global: another script may read it.
    assert_eq!(
        min(MinifyTarget::Html, page),
        "<!doctype html><html lang=en><head><meta charset=utf-8><title>A & B</title></head>\
         <body><p class=x>one two</p><script>var answer=42;console.log(answer);</script>\
         </body></html>"
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
    // `{{x}}` parses as JavaScript (two blocks): a script with template syntax is kept.
    let input = "<script> {{x}} </script>";
    assert_eq!(with(o, MinifyTarget::Html, input), "<script>{{x}}</script>");
}

#[test]
fn html_scripts() {
    // Google Tag Manager's snippet: minify-html's own JS minification printed a longer result
    // (`/* @__PURE__ */` annotations) and kept the snippet with its line breaks.
    let gtm = "<script>(function(w,d,s,l,i){w[l]=w[l]||[];w[l].push({'gtm.start':\n\
               new Date().getTime(),event:'gtm.js'});var f=d.getElementsByTagName(s)[0],\n\
               j=d.createElement(s),dl=l!='dataLayer'?'&l='+l:'';j.async=true;j.src=\n\
               'https://www.googletagmanager.com/gtm.js?id='+i+dl;f.parentNode.insertBefore(j,f);\n\
               })(window,document,'script','dataLayer',\"GTM-0000\");</script>";
    assert_eq!(
        min(MinifyTarget::Html, gtm),
        "<script>(function(e,t,n,r,i){e[r]=e[r]||[],e[r].push({\"gtm.start\":new Date().getTime(),\
         event:`gtm.js`});var a=t.getElementsByTagName(n)[0],o=t.createElement(n),\
         s=r==`dataLayer`?``:`&l=`+r;o.async=!0,o.src=`https://www.googletagmanager.com/gtm.js?id=`\
         +i+s,a.parentNode.insertBefore(o,a)})(window,document,`script`,`dataLayer`,`GTM-0000`);\
         </script>"
    );
    // A classic script's top-level names are globals: kept, even when unused.
    assert_eq!(
        min(
            MinifyTarget::Html,
            "<script>\n  window.dataLayer = window.dataLayer || [];\n  \
             function gtag(){dataLayer.push(arguments);}\n  gtag('js', new Date());\n</script>"
        ),
        "<script>window.dataLayer=window.dataLayer||[];function gtag(){dataLayer.push(arguments)}\
         gtag(`js`,new Date);</script>"
    );
    assert_eq!(
        min(
            MinifyTarget::Html,
            "<script type=module>\nimport { a } from './a.js';\nconst b = a + 1;\nconsole.log(b);\n</script>"
        ),
        "<script type=module>import{a}from\"./a.js\";const b=a+1;console.log(b);</script>"
    );
    // JSON: structured data, import maps; strings are copied.
    assert_eq!(
        min(
            MinifyTarget::Html,
            "<script type=\"application/ld+json\">\n  {\n    \"@context\": \"https://schema.org\",\n    \
             \"name\": \"Snack  Diary\"\n  }\n</script>\
             <SCRIPT TYPE=\"Application/LD+JSON\">{ \"a\" : 1 }</SCRIPT>\
             <script type=importmap>\n{ \"imports\": { \"a\": \"./a.js\" } }\n</script>"
        ),
        "<script type=application/ld+json>{\"@context\":\"https://schema.org\",\"name\":\"Snack  Diary\"}\
         </script><script type=application/ld+json>{\"a\":1}</script>\
         <script type=importmap>{\"imports\":{\"a\":\"./a.js\"}}</script>"
    );
    // Kept as written: other types (client-side templates), content that does not parse, a
    // result that would open an HTML comment, an SVG script (its text is markup), and `<script>`
    // in text, attribute values and comments.
    for kept in [
        "<script type=text/x-tmpl-mustache>\n  <div>{{ title }}</div>\n</script>",
        "<script>var = ;</script>",
        "<script type=application/ld+json>{ \"a\": [1, 2 }</script>",
        "<script>var  s = \"<!--\";</script>",
        "<svg><script>if (a &lt; b) { go( ) }</script></svg>",
        "<textarea><script> go( 1 ) </script></textarea>",
        "<p title=\"<script> x( 1 ) </script>\">x</p>",
    ] {
        assert_eq!(min(MinifyTarget::Html, kept), kept);
    }
    assert_eq!(
        min(
            MinifyTarget::Html,
            "<!-- <script> y( 2 ) </script> --><svg/><script>  z( 3 )  </script>"
        ),
        "<svg/><script>z(3);</script>"
    );
    // Local names are mangled unless `keepVarNames`.
    let local = "<script>function f(input){ return input * 2 }</script>";
    assert_eq!(
        min(MinifyTarget::Html, local),
        "<script>function f(e){return e*2}</script>"
    );
    let mut o = Options::default();
    o.js.keep_var_names = true;
    assert_eq!(
        with(o, MinifyTarget::Html, local),
        "<script>function f(input){return input*2}</script>"
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
    // A `with` body looks names up on an object: nothing is renamed or rewritten.
    assert_eq!(
        min(
            MinifyTarget::Js,
            "function f(obj) {\n  var value = 1;\n  with (obj) { console.log(value) }\n}"
        ),
        "function f(obj){var value=1;with(obj){console.log(value)}}"
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
