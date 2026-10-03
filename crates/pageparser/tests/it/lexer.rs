//! The lexer against Go's (`parser/pageparser/pages.json.gz`): every item of every input in
//! the three configurations the oracle records (page, body only, no summary divider), by kind
//! and byte range, plus the typed values of arguments.

use serde_json::Value as J;
use ssg_pageparser::{
    Delim, FrontMatterFormat, LexOptions, Quoting, Scalar, Start, SummaryDivider, Token, TokenKind,
    lex, lex_with, split_front_matter,
};
use ssg_testkit::fixture::{GoString, Tag};

use crate::support::{Tally, page_cases};

/// The Go implementation's item types (`parser/pageparser/item.go`).
const T_ERROR: i64 = 0;
const T_EOF: i64 = 1;

fn go_type(kind: TokenKind) -> i64 {
    match kind {
        TokenKind::SummaryDivider => 2,
        TokenKind::FrontMatter(FrontMatterFormat::Yaml) => 3,
        TokenKind::FrontMatter(FrontMatterFormat::Toml) => 4,
        TokenKind::FrontMatter(FrontMatterFormat::Json) => 5,
        TokenKind::FrontMatter(FrontMatterFormat::Org) => 6,
        TokenKind::ByteOrderMark => 7,
        TokenKind::LeftDelim(Delim::Html) => 8,
        TokenKind::RightDelim(Delim::Html) => 9,
        TokenKind::LeftDelim(Delim::Markdown) => 10,
        TokenKind::RightDelim(Delim::Markdown) => 11,
        TokenKind::Close => 12,
        TokenKind::Name => 13,
        TokenKind::InlineName => 14,
        TokenKind::Param(_) => 15,
        TokenKind::Value(_) => 16,
        TokenKind::Indentation => 17,
        TokenKind::Text => 18,
    }
}

const CONFIGS: [LexOptions; 3] = [
    LexOptions {
        start: Start::Page,
        summary_divider: SummaryDivider::Html,
    },
    LexOptions {
        start: Start::Body,
        summary_divider: SummaryDivider::Html,
    },
    LexOptions {
        start: Start::Page,
        summary_divider: SummaryDivider::Off,
    },
];

/// Go's segments of an escaped string: the span split at (and without) every backslash.
fn segments(src: &[u8], t: &Token) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut k = t.span.start;
    for i in t.span.clone() {
        if src[i] == b'\\' {
            if i > k {
                out.push((k, i));
            }
            k = i + 1;
        }
    }
    if k < t.span.end {
        out.push((k, t.span.end));
    }
    out
}

/// One Go item: `[type, low, high, firstByte, isString, segments, err, pos, typed, …]`.
struct GoItem<'a>(&'a [J]);

impl GoItem<'_> {
    fn int(&self, i: usize) -> i64 {
        self.0[i].as_i64().expect("integer item field")
    }
    fn range(&self) -> (i64, i64) {
        (self.int(1), self.int(2))
    }
    fn segments(&self) -> Option<Vec<(usize, usize)>> {
        let segs = self.0[5].as_array()?;
        Some(
            segs.iter()
                .map(|s| {
                    let s = s.as_array().expect("segment");
                    (as_usize(&s[0]), as_usize(&s[1]))
                })
                .collect(),
        )
    }
}

fn as_usize(v: &J) -> usize {
    usize::try_from(v.as_u64().expect("offset")).expect("offset")
}

fn to_i64(n: usize) -> i64 {
    i64::try_from(n).expect("offset")
}

fn same_scalar(ours: &Scalar, go: &J) -> Option<bool> {
    if Tag::of(go).is_some() {
        return None; // not UTF-8
    }
    Some(match (ours, go) {
        (Scalar::String(s), J::String(g)) => s == g,
        (Scalar::Bool(b), J::Bool(g)) => b == g,
        (Scalar::Int(i), J::Number(n)) => n.as_i64() == Some(*i) && !n.is_f64(),
        (Scalar::Float(f), J::Number(n)) => n.is_f64() && n.as_f64() == Some(*f),
        _ => false,
    })
}

#[test]
fn lexer_items_match_go() {
    let mut tally = Tally::new("pageparser/lex items");
    let mut typed = Tally::new("pageparser/typed arguments");
    let mut items = 0_usize;
    let cases = page_cases();
    for (id, c) in &cases {
        let src: GoString = serde_json::from_value(c["src"].clone()).expect("src");
        let src = src.0.as_slice();
        for (ci, opts) in CONFIGS.iter().enumerate() {
            let want = c["lex"][ci]["items"].as_array().expect("items");
            let lexed = lex_with(src, *opts);
            let mut got: Vec<(i64, Option<&Token>, (i64, i64))> = lexed
                .tokens
                .iter()
                .map(|t| {
                    (
                        go_type(t.kind),
                        Some(t),
                        (to_i64(t.span.start), to_i64(t.span.end)),
                    )
                })
                .collect();
            match &lexed.error {
                Some(e) => {
                    let at = lexed
                        .tokens
                        .iter()
                        .take_while(|t| t.span.end <= e.span.start)
                        .count();
                    let range = (to_i64(e.span.start), to_i64(e.span.end));
                    got.insert(at, (T_ERROR, None, range));
                    if at < lexed.tokens.len() {
                        got.push((T_EOF, None, (to_i64(src.len()), to_i64(src.len()))));
                    }
                }
                None => got.push((T_EOF, None, (to_i64(src.len()), to_i64(src.len())))),
            }
            items += want.len();
            tally.check(got.len() == want.len(), || {
                format!(
                    "{id} cfg{ci}: {} items, Go {}: {:?}",
                    got.len(),
                    want.len(),
                    lexed
                )
            });
            for (i, ((ty, tok, range), w)) in got.iter().zip(want).enumerate() {
                let w = GoItem(w.as_array().expect("item"));
                let (ok, what) = match tok.map(|t| (t, t.quoting())) {
                    Some((t, Some(Quoting::Escaped))) => {
                        let segs = segments(src, t);
                        (
                            *ty == w.int(0) && Some(segs.clone()) == w.segments(),
                            format!("{segs:?}"),
                        )
                    }
                    Some((_, q)) => (
                        *ty == w.int(0)
                            && *range == w.range()
                            && w.0[5].is_null()
                            && w.0[4].as_bool() == Some(q.is_some_and(|q| q != Quoting::Bare)),
                        format!("{range:?} quoting {q:?}"),
                    ),
                    None => (*ty == w.int(0) && *range == w.range(), format!("{range:?}")),
                };
                tally.check(ok, || {
                    format!("{id} cfg{ci} item {i}: got type {ty} {what}, Go {:?}", w.0)
                });
                if let (Some(t), Ok(s)) = (tok, std::str::from_utf8(src))
                    && t.quoting().is_some()
                {
                    match same_scalar(&t.scalar(s), &w.0[8]) {
                        Some(ok) => typed.check(ok, || {
                            format!("{id} cfg{ci} item {i}: {:?} vs {}", t.scalar(s), w.0[8])
                        }),
                        None => typed.skipped += 1,
                    }
                }
            }
        }
    }
    eprintln!(
        "pageparser/lex: {items} Go items over {} inputs",
        cases.len()
    );
    assert_eq!(items, 135_326);
    assert_eq!(cases.len(), 5_322);
    tally.finish();
    typed.finish();
}

/// `split_front_matter` against Go's `ParseFrontMatterAndContent` (format and body offset),
/// and `lex(body)` after it against the page lexer's tokens after the front matter.
#[test]
fn split_matches_go() {
    let mut split_tally = Tally::new("pageparser/split");
    let mut body_tally = Tally::new("pageparser/body lex after split");
    let diffs = crate::support::expected_diffs();
    let divider_at_start = &diffs["body_after_split"]["divider_at_start"];
    for (id, c) in page_cases() {
        let Some(src) = c["src"].as_str() else {
            split_tally.skipped += 1;
            continue;
        };
        let fm = &c["fm"];
        let split = split_front_matter(src);
        let want_format = fm["format"].as_str().expect("format");
        let got_format = match &split {
            Ok(s) => s.front_matter.map_or("", |(f, _)| f.name()),
            Err(_) => "",
        };
        let got_offset = split
            .as_ref()
            .ok()
            .filter(|s| s.front_matter.is_some())
            .map(|s| s.body_offset as u64);
        split_tally.check(
            got_format == want_format && got_offset == fm["content"].as_u64(),
            || format!("{id}: {got_format} {got_offset:?}, Go {fm}"),
        );

        // The body lexed alone equals the page lexer after the front matter.
        let Ok(split) = split else { continue };
        let page = lex_with(src.as_bytes(), CONFIGS[0]);
        let expect: Vec<Token> = page
            .tokens
            .iter()
            .filter(|t| t.span.start >= split.body_offset)
            .filter(|t| !matches!(t.kind, TokenKind::FrontMatter(_) | TokenKind::ByteOrderMark))
            .cloned()
            .collect();
        let expect_err = page.error.as_ref().map(|e| e.span.clone());
        let shift = |t: Token| Token {
            kind: t.kind,
            span: t.span.start + split.body_offset..t.span.end + split.body_offset,
        };
        let divider = match split.front_matter {
            Some((FrontMatterFormat::Org, _)) => SummaryDivider::Org,
            _ => SummaryDivider::Html,
        };
        let body = lex_with(
            split.body.as_bytes(),
            LexOptions {
                start: Start::Body,
                summary_divider: divider,
            },
        );
        let got: Vec<Token> = body.tokens.into_iter().map(shift).collect();
        let got_err = body
            .error
            .map(|e| e.span.start + split.body_offset..e.span.end + split.body_offset);
        let ok = got == expect && got_err == expect_err;
        let starts_with_divider = split.front_matter.is_none()
            && split
                .body
                .trim_start_matches([' ', '\t', '\r', '\n'])
                .starts_with("<!--more-->");
        if !ok && starts_with_divider {
            body_tally.deviation(format!("{id}: {divider_at_start}"));
        } else {
            body_tally.check(ok, || {
                format!("{id}: body {got:?} {got_err:?}\n  page {expect:?} {expect_err:?}")
            });
        }
    }
    split_tally.finish();
    body_tally.finish();
}

#[test]
fn lex_body_api() {
    let src = "a {{< x 1 >}}";
    let toks = lex(src).unwrap();
    assert_eq!(toks[0].text(src), "a ");
    assert_eq!(toks[3].scalar(src), Scalar::Int(1));
}
