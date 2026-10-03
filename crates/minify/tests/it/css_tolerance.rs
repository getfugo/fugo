//! CSS lightningcss rejects is passed through, not an error (as Go's tdewolff minifier):
//! accepted rules are still minified, rejected ones keep their text without comments and
//! redundant whitespace.

use ssg_minify::{Minifier, MinifyTarget};
use ssg_testkit::fixture::testdata;

/// Minifies `input` and checks that the result is a fixed point.
fn min(input: &str) -> String {
    let m = Minifier::default();
    let out = m
        .minify(MinifyTarget::Css, input)
        .unwrap_or_else(|e| panic!("{e}: {input}"))
        .into_owned();
    let again = m
        .minify(MinifyTarget::Css, &out)
        .expect("again")
        .into_owned();
    assert_eq!(again, out, "not idempotent for {input:?}");
    out
}

/// The Tailwind source of the docs site (`@media screen(md)`, `@apply`, `@theme`): lightningcss
/// rejects `screen(md)` as a media query, which failed the docs build.
#[test]
fn docs_tailwind_source() {
    let path = ssg_testkit::fixture::legacy_docs().join("assets/css/styles.css");
    let input = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
    assert!(input.contains("@media screen(md) {"), "fixture changed");
    let out = min(&input);
    for want in [
        // Rejected group rules: the prelude as written, the rules inside minified.
        "@media screen(md){.px-main{padding-left:max(env(safe-area-inset-left), 2rem);\
         padding-right:max(env(safe-area-inset-right), 2rem)}}",
        "@media screen(lg){.px-main{",
        // Accepted rules around them: minified by lightningcss (declarations kept as written).
        "html{scroll-padding-top:100px}",
        ".footnote-backref,.footnote-ref{text-decoration:none;padding-left:.0625em}",
        "@import \"tailwindcss\";@plugin \"@tailwindcss/typography\";",
        "body{@apply antialiased font-sans text-black dark:text-gray-100;}",
    ] {
        assert!(out.contains(want), "{want} not in {out}");
    }
    assert!(!out.contains("/*") && !out.contains('\n'), "{out}");
}

#[test]
fn rejected_rules_pass_through() {
    // A stray token in a declaration: the rule as written, its neighbours minified.
    assert_eq!(
        min(
            "a { color : #ff0000 }\nb { x: ) ; content: \"  s  \" ; background: url( a b.png ) }\nc { margin: 0px 0px }"
        ),
        "a{color:red}b{x:);content: \"  s  \";background: url( a b.png )}c{margin:0}"
    );
    // A selector lightningcss rejects: its declarations are still minified.
    assert_eq!(
        min("a!b , c { color : #ff0000 ; margin: 0px 0px 0px 0px }"),
        "a!b,c{color:red;margin:0}"
    );
    // `@import` after a rule (tdewolff keeps it).
    assert_eq!(
        min(".a { color: red; }\n@import \"nope.css\";"),
        ".a{color:red}@import \"nope.css\";"
    );
    // A rejected rule inside a rejected group rule.
    assert_eq!(
        min("@media screen(md) {\n  a { b: ) }\n  c { color: #ff0000 }\n}\nd{top:0px}"),
        "@media screen(md){a{b:)}c{color:red}}d{top:0}"
    );
    // Comments go, `/*!` comments stay; a comment that separates two tokens stays as `/**/`.
    assert_eq!(
        min("/* x */ a /*! keep  me */ { b : ) } /* y */ c/**/d { e: ) }"),
        "a /*! keep  me */{b :)}c/**/d{e:)}"
    );
    // An unclosed rejected block at the end.
    assert_eq!(min("a { b: )  ;  c : d"), "a{b:);c : d");
    assert_eq!(
        min("@media screen(md) { a { color: #ff0000 }"),
        "@media screen(md){a{color:red}}"
    );
}

/// The CSS inputs of the oracle sites: all accepted by lightningcss, so unchanged by the
/// tolerant path (`imports-missing.css`, `@import` after a rule, was an error before).
#[test]
fn oracle_site_css() {
    let mut failures = Vec::new();
    for (file, want) in [
        (
            "resource-transformers/t16site/assets/css/plain.css",
            ".plain{color:red}.two{margin:0;color:#00f}",
        ),
        (
            "resource-transformers/t16site/assets/css/broken.css",
            ".ok{color:red}.broken{color:red}",
        ),
        (
            "resource-transformers/t16site/assets/css/imports-missing.css",
            ".a{color:red}@import \"nope.css\";",
        ),
        (
            "resource-transformers/t16site/assets/css/imports.css",
            "@import \"plain.css\";@import \"sub/x.css\";@import \"remote.css\";\
             @import \"print.css\" print;@import \"tailwindcss\";@import \"sub/x.css\";\
             .after{color:red}",
        ),
        (
            "resource-transformers/t16site/assets/css/imports-comment.css",
            "@import \"plain.css\";",
        ),
        (
            "resource-transformers/t16site/assets/css/imports-broken.css",
            "@import \"plain.css\";@import \"broken.css\";",
        ),
        (
            "resource-transformers/t16site/assets/css/sub/x.css",
            "@import \"y.css\";.x{color:red}",
        ),
        (
            "resource-transformers/t16site/assets/js/style.css",
            ".a{color:red}",
        ),
        (
            "resource-transformers/site/assets/css/a.css",
            "body{margin:.5em;color:red}",
        ),
        (
            "resource-transformers/site/assets/css/b.css",
            ".b{padding:10px 0;background:url(\"data:image/svg+xml;charset=utf-8,%3Csvg \
             xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Cpath d='M 1 1 L 2 \
             2'/%3E%3C/svg%3E\")}",
        ),
        (
            "resources/site/assets/css/a.css",
            "body{color:red;margin:0}",
        ),
        ("resources/site/assets/css/b.css", ".b{padding:1.5em}"),
        (
            "resources/site/content/blog/bundle1/style.css",
            "p{color:#00f}",
        ),
    ] {
        let path = testdata(&format!("oracle/{file}"));
        let input = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
        let got = min(&input);
        if got != want {
            failures.push(format!("{file}: {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Many rejected rules: each rule is tried once on its own, so the cost stays linear.
#[test]
fn many_rejected_rules() {
    let rule = "@media screen(md) { .a { color: #ff0000 } }\n.b { color : #0000ff }\n";
    let input = rule.repeat(2000);
    let start = std::time::Instant::now();
    let out = min(&input);
    assert_eq!(
        out,
        "@media screen(md){.a{color:red}}.b{color:#00f}".repeat(2000)
    );
    assert!(start.elapsed().as_secs() < 30, "{:?}", start.elapsed());
    // Accepted one by one, rejected together: the `@import` after a rule, amid others. Rules
    // are not merged (that is the source's business, as for Go's minifier).
    assert_eq!(
        min(&format!(
            "{}@import 'x.css';{}",
            ".a{top:0px}".repeat(50),
            ".b{top:0px}".repeat(50)
        )),
        format!(
            "{}@import 'x.css';{}",
            ".a{top:0}".repeat(50),
            ".b{top:0}".repeat(50)
        )
    );
}

/// Fallback declarations survive: a standard value followed by a `-webkit-` one that browsers
/// reject (`circle at …` is not legacy `-webkit-radial-gradient` syntax), as in a site's
/// Instagram icon. Merging them into the last one left the icon without a background.
#[test]
fn fallback_declarations_are_kept() {
    let out = min(
        ".i{background:radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%);\
         background:-webkit-radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%)}",
    );
    assert!(
        out.contains("background:radial-gradient(")
            && out.contains("background:-webkit-radial-gradient("),
        "{out}"
    );
    let out = min(".c{color:#123456;color:color(display-p3 .1 .2 .3)}");
    assert!(
        out.contains("color:#123456") && out.contains("color:color(display-p3"),
        "{out}"
    );
}
