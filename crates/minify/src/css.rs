//! Stand-alone CSS through lightningcss, tolerant of CSS lightningcss rejects.
//!
//! The Go implementation's minifier (tdewolff) never fails on CSS: it minifies what it
//! understands and writes everything else out as it is (Tailwind sources with
//! `@media screen(md)`, declarations with stray tokens, `@import` after rules, …). Here a style
//! sheet lightningcss accepts is minified as a whole. Otherwise it is cut into its top-level
//! rules ([`split`]); each maximal run of rules lightningcss accepts is minified by
//! lightningcss, and each rule it rejects is rescued ([`rescue`]): the body of a conditional
//! group rule (`@media`, `@supports`, …) and the declarations of a style rule are minified on
//! their own where possible, and whatever is left is only stripped of comments and redundant
//! whitespace ([`fallback`]).
//!
//! Without browser targets lightningcss only parses and prints compactly, and declarations and
//! rules stay as written (as Go's minifier keeps them). With targets (the project's browserslist
//! configuration, [`crate::project_browsers`]) its `minify` pass also runs: it adds the vendor
//! prefixes and lowers the syntax those browsers need (autoprefixer's job) and merges rules and
//! declarations. That pass keeps only the last of two declarations of one property, assuming every
//! browser supports every value it parses, which drops hand-written fallbacks such as a standard
//! `radial-gradient(circle at …)` followed by a `-webkit-radial-gradient` browsers reject. So a
//! style rule declaring a property twice is not merged: it counts as rejected and is rescued as
//! written, while the rules around it are minified with the targets.

use lightningcss::declaration::DeclarationBlock;
use lightningcss::properties::PropertyId;
use lightningcss::rules::{CssRule, CssRuleList};
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::targets::{Features, Targets};

use crate::options::CssOptions;

/// Minifies `input`; never fails (as the Go implementation's minifier).
pub(crate) fn minify(o: &CssOptions, input: &str) -> String {
    whole(o, input).unwrap_or_else(|| tolerant(o, input))
}

/// lightningcss can reorder `!important` declarations it keeps unparsed differently when it
/// minifies its own output; passes repeat (at most three) until the output is stable. `None` if
/// lightningcss rejects `input`.
fn whole(o: &CssOptions, input: &str) -> Option<String> {
    let mut out = pass(o, input)?;
    for _ in 0..2 {
        match pass(o, &out) {
            Some(again) if again != out => out = again,
            _ => break,
        }
    }
    Some(out)
}

/// One lightningcss pass over `input`; `None` if lightningcss rejects it, or, with browser
/// targets, if a style rule in it declares a property twice (see the module documentation).
fn pass(o: &CssOptions, input: &str) -> Option<String> {
    let targets = targets(o);
    let mut sheet = StyleSheet::parse(input, ParserOptions::default()).ok()?;
    if o.browsers.is_some() {
        if has_fallbacks(&sheet.rules) {
            return None;
        }
        sheet
            .minify(MinifyOptions {
                targets,
                ..MinifyOptions::default()
            })
            .ok()?;
    }
    let printed = sheet
        .to_css(PrinterOptions {
            minify: true,
            targets,
            ..PrinterOptions::default()
        })
        .ok()?;
    Some(printed.code)
}

/// The lightningcss targets of `o`: its browsers, and CSS 2/3 colour syntax with `keep_css2`.
pub(crate) fn targets(o: &CssOptions) -> Targets {
    Targets {
        browsers: o.browsers,
        include: if o.keep_css2 {
            Features::HexAlphaColors | Features::SpaceSeparatedColorNotation
        } else {
            Features::empty()
        },
        exclude: Features::empty(),
    }
}

/// Whether a style rule of `rules` (or of a group or nested rule) declares a property twice.
fn has_fallbacks<R>(rules: &CssRuleList<'_, R>) -> bool {
    rules.0.iter().any(|rule| match rule {
        CssRule::Style(r) => declares_twice(&r.declarations) || has_fallbacks(&r.rules),
        CssRule::Media(r) => has_fallbacks(&r.rules),
        CssRule::Supports(r) => has_fallbacks(&r.rules),
        CssRule::Container(r) => has_fallbacks(&r.rules),
        CssRule::LayerBlock(r) => has_fallbacks(&r.rules),
        CssRule::MozDocument(r) => has_fallbacks(&r.rules),
        CssRule::Scope(r) => has_fallbacks(&r.rules),
        CssRule::StartingStyle(r) => has_fallbacks(&r.rules),
        CssRule::Nesting(r) => {
            declares_twice(&r.style.declarations) || has_fallbacks(&r.style.rules)
        }
        _ => false,
    })
}

fn declares_twice(block: &DeclarationBlock<'_>) -> bool {
    [&block.declarations, &block.important_declarations]
        .into_iter()
        .any(|decls| {
            let ids: Vec<PropertyId<'_>> = decls
                .iter()
                .map(lightningcss::properties::Property::property_id)
                .collect();
            ids.iter().enumerate().any(|(i, id)| ids[..i].contains(id))
        })
}

/// A style sheet lightningcss rejects: each top-level rule is tried on its own, runs of accepted
/// rules go through lightningcss together, rejected rules are rescued.
fn tolerant(o: &CssOptions, input: &str) -> String {
    let chunks = split(input);
    let text = |from: usize, to: usize| -> &str {
        if from == to {
            ""
        } else {
            &input[chunks[from].0..chunks[to - 1].1]
        }
    };
    let accepted: Vec<bool> = (0..chunks.len())
        .map(|i| pass(o, text(i, i + 1)).is_some())
        .collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chunks.len() {
        if !accepted[at] {
            out.push_str(&rescue(o, text(at, at + 1)));
            at += 1;
            continue;
        }
        let end = (at..chunks.len())
            .find(|&i| !accepted[i])
            .unwrap_or(chunks.len());
        run(o, &text, at, end, &mut out);
        at = end;
    }
    out
}

/// Rules `from..to`, each accepted on its own. Rejected together (an `@import` after a style
/// rule), the rule that makes the difference is rescued: the shortest rejected prefix is found by
/// bisection.
fn run<'a>(
    o: &CssOptions,
    text: &impl Fn(usize, usize) -> &'a str,
    mut from: usize,
    to: usize,
    out: &mut String,
) {
    while from < to {
        if let Some(s) = whole(o, text(from, to)) {
            out.push_str(&s);
            return;
        }
        // `text(from, lo)` is accepted (or empty), `text(from, hi)` is not.
        let (mut lo, mut hi) = (from, to);
        let mut prefix = String::new();
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            match whole(o, text(from, mid)) {
                Some(s) => {
                    lo = mid;
                    prefix = s;
                }
                None => hi = mid,
            }
        }
        out.push_str(&prefix);
        out.push_str(&rescue(o, text(lo, hi)));
        from = hi;
    }
}

/// At-rules whose block holds rules, like a style sheet.
const GROUP_RULES: [&str; 8] = [
    "media",
    "supports",
    "container",
    "layer",
    "document",
    "-moz-document",
    "scope",
    "starting-style",
];

/// One top-level rule lightningcss rejects.
fn rescue(o: &CssOptions, rule: &str) -> String {
    let Some((open, close)) = block(rule) else {
        return fallback(rule);
    };
    let prelude = &rule[..open];
    let body = &rule[open + 1..close.unwrap_or(rule.len())];
    let after = close.map_or("", |c| &rule[c + 1..]);
    let head = prelude.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let mut out = fallback(prelude);
    if let Some(name) = head.strip_prefix('@') {
        let name = name
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .next()
            .unwrap_or_default();
        if !GROUP_RULES.iter().any(|g| g.eq_ignore_ascii_case(name)) {
            return fallback(rule);
        }
        out.push('{');
        out.push_str(&minify(o, body));
    } else {
        // A style rule whose selector lightningcss rejects: its declarations under `*`.
        let wrapped = format!("*{{{body}}}");
        match whole(o, &wrapped) {
            Some(s) if s.is_empty() => out.push('{'),
            Some(s) if s.starts_with("*{") && block(&s) == Some((1, Some(s.len() - 1))) => {
                out.push_str(&s[1..s.len() - 1]);
            }
            _ => return fallback(rule),
        }
    }
    out.push('}');
    out.push_str(&fallback(after));
    out
}

/// What starts at `b[i]` and is copied (or dropped) as a whole.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Skip {
    Comment,
    PreservedComment,
    /// A string, an escape or an unquoted `url(…)`.
    Verbatim,
}

fn is_name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b'\\' || c >= 0x80
}

/// The end of the comment, string, escape or unquoted `url(…)` starting at `i`, if one does.
fn skip(s: &str, i: usize) -> Option<(usize, Skip)> {
    let b = s.as_bytes();
    let find = |from: usize, pat: &str| s[from..].find(pat).map(|p| from + p);
    match b[i] {
        b'/' if b.get(i + 1) == Some(&b'*') => {
            let end = find(i + 2, "*/").map_or(s.len(), |p| p + 2);
            let kind = if b.get(i + 2) == Some(&b'!') {
                Skip::PreservedComment
            } else {
                Skip::Comment
            };
            Some((end, kind))
        }
        q @ (b'"' | b'\'') => {
            let mut j = i + 1;
            while j < b.len() {
                match b[j] {
                    b'\\' => j = next_char(s, j + 1),
                    // An unterminated string ends at the newline (kept, so that it stays one).
                    b'\n' | b'\r' | b'\x0c' => return Some((j + 1, Skip::Verbatim)),
                    c if c == q => return Some((j + 1, Skip::Verbatim)),
                    _ => j += 1,
                }
            }
            Some((b.len(), Skip::Verbatim))
        }
        b'\\' => Some((next_char(s, i + 1), Skip::Verbatim)),
        b'u' | b'U'
            if b.len() >= i + 4
                && b[i..i + 4].eq_ignore_ascii_case(b"url(")
                && (i == 0 || !is_name_byte(b[i - 1])) =>
        {
            let mut j = i + 4;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if matches!(b.get(j), Some(b'"' | b'\'')) {
                return None;
            }
            while j < b.len() {
                match b[j] {
                    b'\\' => j = next_char(s, j + 1),
                    b')' => return Some((j + 1, Skip::Verbatim)),
                    _ => j += 1,
                }
            }
            Some((b.len(), Skip::Verbatim))
        }
        _ => None,
    }
}

/// The char boundary after the character at `i` (or the end).
fn next_char(s: &str, i: usize) -> usize {
    s[i.min(s.len())..]
        .chars()
        .next()
        .map_or(s.len(), |c| i + c.len_utf8())
}

fn closer(c: u8) -> Option<u8> {
    match c {
        b'{' => Some(b'}'),
        b'(' => Some(b')'),
        b'[' => Some(b']'),
        _ => None,
    }
}

/// The top-level rules of a style sheet as byte ranges that cover it: a rule ends after the `}`
/// of its block or, for an at-rule, after its `;`. Comments and whitespace go with the next rule.
pub(crate) fn split(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut chunks = Vec::new();
    let (mut start, mut i) = (0, 0);
    let mut stack = Vec::new();
    let mut first: Option<u8> = None;
    while i < b.len() {
        if let Some((end, kind)) = skip(s, i) {
            if kind == Skip::Verbatim && first.is_none() {
                first = Some(b[i]);
            }
            i = end;
            continue;
        }
        let c = b[i];
        i += 1;
        if first.is_none() && !c.is_ascii_whitespace() {
            first = Some(c);
        }
        let end = if let Some(close) = closer(c) {
            stack.push(close);
            false
        } else if matches!(c, b'}' | b')' | b']') {
            if stack.last() == Some(&c) {
                stack.pop();
                c == b'}' && stack.is_empty()
            } else {
                // A stray `}` ends a rule; other stray closers are part of it.
                c == b'}' && stack.is_empty()
            }
        } else {
            c == b';' && stack.is_empty() && first == Some(b'@')
        };
        if end {
            chunks.push((start, i));
            start = i;
            first = None;
        }
    }
    if start < b.len() {
        chunks.push((start, b.len()));
    }
    chunks
}

/// The top-level `{` of a rule and the `}` that closes it (`None` when the block is unclosed).
fn block(rule: &str) -> Option<(usize, Option<usize>)> {
    let b = rule.as_bytes();
    let mut stack = Vec::new();
    let mut open = None;
    let mut i = 0;
    while i < b.len() {
        if let Some((end, _)) = skip(rule, i) {
            i = end;
            continue;
        }
        let c = b[i];
        if let Some(close) = closer(c) {
            if c == b'{' && stack.is_empty() && open.is_none() {
                open = Some(i);
            }
            stack.push(close);
        } else if stack.last() == Some(&c) {
            stack.pop();
            if stack.is_empty() && c == b'}' {
                return open.map(|o| (o, Some(i)));
            }
        }
        i += 1;
    }
    open.map(|o| (o, None))
}

/// No space is needed next to these (on either side).
fn tight(c: u8) -> bool {
    matches!(c, b'{' | b'}' | b';' | b',')
}

/// A comment between `prev` and `next` can go without joining them into one token.
fn separate(prev: u8, next: u8) -> bool {
    matches!(
        prev,
        b'{' | b'}' | b';' | b',' | b':' | b'[' | b']' | b'(' | b')'
    ) || matches!(next, b'{' | b'}' | b';' | b',' | b':' | b'[' | b']' | b')')
}

/// The conservative minifier for CSS lightningcss rejects: comments go (except `/*! … */`),
/// whitespace collapses to one space and goes next to `{`, `}`, `;` and `,`, after `(` and before
/// `)`. Strings, escapes and unquoted `url(…)` are copied as they are.
fn fallback(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    let mut comment = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            space = true;
            i += 1;
            continue;
        }
        let skipped = skip(s, i);
        if let Some((end, Skip::Comment)) = skipped {
            comment = true;
            i = end;
            continue;
        }
        let prev = out.as_bytes().last().copied();
        if let Some(p) = prev {
            if space && !tight(p) && !tight(c) && p != b'(' && c != b')' {
                out.push(' ');
            } else if comment && !space && !separate(p, c) {
                out.push_str("/**/");
            }
        }
        space = false;
        comment = false;
        let end = skipped.map_or_else(|| next_char(s, i), |(end, _)| end);
        out.push_str(&s[i..end]);
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
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
}
