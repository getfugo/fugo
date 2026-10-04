use super::*;

fn chunks(s: &str) -> Vec<&str> {
    split(s).into_iter().map(|(a, b)| &s[a..b]).collect()
}

#[test]
fn split_rules() {
    assert_eq!(
        chunks("@import 'a;b';/*c*/ a{b:c} @media x{a{}} d"),
        ["@import 'a;b';", "/*c*/ a{b:c}", " @media x{a{}}", " d"]
    );
    assert_eq!(chunks("a;b{c:d}"), ["a;b{c:d}"]);
    assert_eq!(chunks("a{b:url(x}y)} }c{"), ["a{b:url(x}y)}", " }", "c{"]);
    assert_eq!(chunks("@x (a;b); @y [}];"), ["@x (a;b);", " @y [}];"]);
    assert_eq!(
        chunks("a{content:'}'} b{content:\"\\\"}\"}"),
        ["a{content:'}'}", " b{content:\"\\\"}\"}"]
    );
    assert_eq!(chunks("a\\{{}b{}"), ["a\\{{}", "b{}"]);
    assert_eq!(chunks(""), Vec::<&str>::new());
}

#[test]
fn fallback_whitespace_and_comments() {
    assert_eq!(
        fallback("  a  >  b ,\n c  {  x : y  ;  }  "),
        "a > b,c{x : y;}"
    );
    assert_eq!(
        fallback("@media screen and ( min-width : 1px ) , print"),
        "@media screen and (min-width : 1px),print"
    );
    assert_eq!(fallback("a /* c */ b"), "a b");
    assert_eq!(fallback("a/* c */b"), "a/**/b");
    assert_eq!(fallback("a/* c */{b}"), "a{b}");
    assert_eq!(fallback("/* c */a/* d */"), "a");
    assert_eq!(fallback("a /*! keep  me */ b"), "a /*! keep  me */ b");
    assert_eq!(fallback("calc(1px  +  2px)"), "calc(1px + 2px)");
}

#[test]
fn fallback_copies_strings_urls_and_escapes() {
    assert_eq!(
        fallback("a{content:'  /* x */  '}"),
        "a{content:'  /* x */  '}"
    );
    assert_eq!(fallback("x:\"a\\\"  b\""), "x:\"a\\\"  b\"");
    assert_eq!(fallback("url( a /*b*/ ;c )  x"), "url( a /*b*/ ;c ) x");
    assert_eq!(fallback("URL( 'a  b' )"), "URL('a  b')");
    assert_eq!(fallback("myurl(a  b)"), "myurl(a b)");
    assert_eq!(fallback("a\\  b"), "a\\  b");
    assert_eq!(fallback("'unterminated  \n  b"), "'unterminated  \n b");
    assert_eq!(fallback("é  ü"), "é ü");
}

#[test]
fn fallback_is_idempotent() {
    for s in [
        "a/* c */b",
        "  a  >  b ,\n c  {  x : y  ;  }  ",
        "url( a /*b*/ ;c )  x",
        "'unterminated  \n  b",
        "a /*! keep */ b",
    ] {
        let once = fallback(s);
        assert_eq!(fallback(&once), once, "{s:?}");
    }
}

#[test]
fn blocks() {
    assert_eq!(block("@media x{a{}}"), Some((8, Some(12))));
    assert_eq!(block("a[x='{']{b"), Some((8, None)));
    assert_eq!(block("@import x;"), None);
}
