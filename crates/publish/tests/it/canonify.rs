//! The canonify rewriter against the Go oracle `transform/absurl/cases` (the absURL and
//! absURLInXML replacers: upstream tables, the output-publishing spec vectors, a prefix ×
//! quote × value × suffix grid and random documents).

use serde::Deserialize;
use ssg_publish::{Quoting, UrlRewriter};
use ssg_testkit::fixture::{GoString, oracle_lines};

use crate::support::{Tally, show};

#[derive(Deserialize)]
struct Case {
    #[serde(rename = "in")]
    input: GoString,
    k: String,
    p: GoString,
    out: Option<GoString>,
    panic: Option<String>,
}

const PREFIXES: [&[u8]; 5] = [b"src=", b"href=", b"url=", b"action=", b"srcset="];

fn quotes(q: Quoting) -> [&'static [u8]; 2] {
    match q {
        Quoting::Html => [b"\"", b"'"],
        Quoting::Xml => [b"&#34;", b"&#39;"],
    }
}

fn skip_quote(s: &[u8], q: Quoting) -> Option<&[u8]> {
    quotes(q)
        .into_iter()
        .find_map(|quote| s.strip_prefix(quote))
}

fn root_relative(s: &[u8]) -> bool {
    matches!(s, [b'/', next, ..] if *next != b'/')
}

/// Go treats position 0 as a candidate of every prefix (its prefix positions start at 0, not
/// "unknown"), so a document that starts with `/x` or `"/x` is rewritten there, several times.
fn leading_candidate(input: &[u8], q: Quoting) -> bool {
    root_relative(input) || skip_quote(input, q).is_some_and(root_relative)
}

/// Go remembers where each prefix occurs next and does not look again after a rewrite has
/// skipped past that position, so a prefix inside a rewritten `srcset` list (or inside the
/// dropped root) sends it back into already written input.
fn prefix_inside_rewrite(input: &[u8], q: Quoting) -> bool {
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle);
    (0..input.len()).any(|i| {
        let Some(rest) = input[i..].strip_prefix(b"srcset=".as_slice()) else {
            return false;
        };
        let Some(quote) = quotes(q).into_iter().find(|quote| rest.starts_with(quote)) else {
            return false;
        };
        let value = &rest[quote.len()..];
        if !root_relative(value) {
            return false;
        }
        let Some(close) = find(value, quote) else {
            return false;
        };
        let section = &value[..(close + 1).min(value.len())];
        PREFIXES.iter().any(|p| find(section, p).is_some())
    })
}

#[test]
fn absurl_oracle() {
    let cases: Vec<Case> = oracle_lines("oracle/transform/absurl/cases.jsonl.gz");
    assert!(cases.len() > 90_000, "{} cases", cases.len());
    let mut t = Tally::default();
    for c in &cases {
        let q = if c.k == "html" {
            Quoting::Html
        } else {
            Quoting::Xml
        };
        let prefix = c.p.as_str().expect("the oracle's prefixes are UTF-8");
        let got = UrlRewriter::new(prefix).rewrite(&c.input.0, q);
        match (&c.out, &c.panic) {
            (_, Some(_)) => t.accept("go-panic"),
            (Some(want), None) if *got == *want.0 => t.pass(),
            (Some(_), None) if leading_candidate(&c.input.0, q) => t.accept("leading-candidate"),
            (Some(_), None) if prefix_inside_rewrite(&c.input.0, q) => {
                t.accept("prefix-inside-rewrite");
            }
            (Some(want), None) => t.fail(|| {
                format!(
                    "{} {prefix:?} in={} got={} want={}",
                    c.k,
                    show(&c.input),
                    show(&GoString(got.to_vec())),
                    show(want)
                )
            }),
            (None, None) => panic!("a case without out or panic"),
        }
    }
    t.finish("absurl");
}

/// The upstream `absurlreplacer_test.go` expectations.
#[test]
fn upstream_tables() {
    let rw = |q, prefix: &str, s: &str| UrlRewriter::new(prefix).rewrite_str(s, q).into_owned();
    let base = "http://base/";
    assert_eq!(
        rw(
            Quoting::Html,
            base,
            "<script src=\"/barfoo.js\"></script><a href='/foobar'>"
        ),
        "<script src=\"http://base/barfoo.js\"></script><a href='http://base/foobar'>"
    );
    assert_eq!(
        rw(Quoting::Html, base, "Link: <a href=/asdf   >ASDF</a>"),
        "Link: <a href=http://base/asdf   >ASDF</a>"
    );
    assert_eq!(
        rw(
            Quoting::Html,
            "../../",
            r#"PRE. a href="/img/small.jpg" input action="/foo.html" meta url=/redirect/to/page/ POST."#
        ),
        r#"PRE. a href="../../img/small.jpg" input action="../../foo.html" meta url=../../redirect/to/page/ POST."#
    );
    assert_eq!(
        rw(
            Quoting::Xml,
            base,
            "Pre. <img srcset=&#34;/img/small.jpg 200w, /img/big.jpg 700w&#34; alt=&#34;text&#34; src=&#34;/img/foo.jpg&#34;>"
        ),
        "Pre. <img srcset=&#34;http://base/img/small.jpg 200w, http://base/img/big.jpg 700w&#34; alt=&#34;text&#34; src=&#34;http://base/img/foo.jpg&#34;>"
    );
}
