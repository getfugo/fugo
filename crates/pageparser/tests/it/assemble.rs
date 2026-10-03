//! `assemble`: shortcode calls, arguments, inner content and `InnerUse`.

use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use ssg_pageparser::{
    Body, Closing, Delim, InnerUse, ParseError, Scalar, Segment, ShortcodeArgs, ShortcodeCall,
    TokenKind, assemble, lex, parse_body, split_front_matter,
};

use crate::support::{Tally, page_cases};

/// `inner` and `outer` use their inner content, `plain` and `youtube` do not; anything else is
/// unknown.
fn oracle(name: &str) -> InnerUse {
    match name {
        "inner" | "outer" | "details" => InnerUse::Required,
        "plain" | "youtube" => InnerUse::Unused,
        _ => InnerUse::UnknownShortcode,
    }
}

fn parse(src: &str) -> Result<Body<'_>, ParseError> {
    parse_body(src, &oracle)
}

fn call<'b, 'a>(seg: &'b Segment<'a>) -> &'b ShortcodeCall<'a> {
    match seg {
        Segment::Shortcode(c) => c,
        Segment::Text(t) => panic!("text {t:?}, not a shortcode"),
    }
}

#[test]
fn open_call_with_typed_positional_arguments() {
    let src = r#"Hi {{< youtube abc-DEF 42 -1.5 true "7" `raw` 1e3 0.125.0 >}} there"#;
    let body = parse(src).unwrap();
    assert_eq!(body.segments.len(), 3);
    assert_eq!(body.segments[0], Segment::Text("Hi "));
    assert_eq!(body.segments[2], Segment::Text(" there"));
    let c = call(&body.segments[1]);
    assert_eq!(c.name, "youtube");
    assert_eq!(c.delim, Delim::Html);
    assert_eq!(c.closing, Closing::Open);
    assert_eq!(&src[c.span.clone()], &src[3..src.len() - 6]);
    assert_eq!(
        c.args,
        ShortcodeArgs::Positional(vec![
            Scalar::String("abc-DEF".into()),
            Scalar::Int(42),
            Scalar::Float(-1.5),
            Scalar::Bool(true),
            Scalar::String("7".into()),
            Scalar::String("raw".into()),
            Scalar::String("1e3".into()),
            Scalar::String("0.125.0".into()),
        ])
    );
}

#[test]
fn named_arguments_keep_order_and_last_value() {
    let body = parse(r#"{{% plain b="x \"y\"" a=1 b=`z` %}}"#).unwrap();
    let c = call(&body.segments[0]);
    assert_eq!(c.delim, Delim::Markdown);
    assert_eq!(
        c.args,
        ShortcodeArgs::Named(vec![
            ("b".into(), Scalar::String("z".into())),
            ("a".into(), Scalar::Int(1)),
        ])
    );
}

#[test]
fn inner_content_nesting_ordinals_and_indentation() {
    let src = "A\n{{< outer x >}}\n  {{< plain 1 >}}\n  {{< inner >}}deep{{< /inner >}}\n{{< /outer >}}\n  {{< plain 2 >}}";
    let body = parse(src).unwrap();
    assert_eq!(body.segments.len(), 4, "{body:#?}");
    assert_eq!(body.segments[0], Segment::Text("A\n"));
    let outer = call(&body.segments[1]);
    assert_eq!(outer.ordinal, 0);
    let Closing::Closed { inner, span } = &outer.closing else {
        panic!("{outer:?}")
    };
    assert_eq!(
        &src[span.clone()],
        "\n  {{< plain 1 >}}\n  {{< inner >}}deep{{< /inner >}}\n"
    );
    assert!(src[outer.span.clone()].ends_with("{{< /outer >}}"));
    // Text, plain, text, inner, text; ordinals count within the parent.
    assert_eq!(inner.len(), 5, "{inner:#?}");
    assert_eq!(inner[0], Segment::Text("\n  "));
    let plain = call(&inner[1]);
    assert_eq!((plain.ordinal, plain.indentation), (0, "  "));
    let nested = call(&inner[3]);
    assert_eq!((nested.name, nested.ordinal), ("inner", 1));
    assert!(
        matches!(&nested.closing, Closing::Closed { inner, .. } if inner == &[Segment::Text("deep")])
    );
    // Adjacent text and indentation form one segment; top-level ordinals ignore nested calls.
    assert_eq!(body.segments[2], Segment::Text("\n  "));
    let second = call(&body.segments[3]);
    assert_eq!((second.ordinal, second.indentation), (1, "  "));
}

#[test]
fn inner_use_decides_where_a_call_ends() {
    // `plain` ends at its `>}}`: the following text is not its inner content.
    let body = parse("{{< plain >}}text").unwrap();
    assert_eq!(call(&body.segments[0]).closing, Closing::Open);
    assert_eq!(body.segments[1], Segment::Text("text"));

    // `inner` must be closed or self-closed.
    assert_eq!(
        call(&parse("{{< inner />}}").unwrap().segments[0]).closing,
        Closing::SelfClosed
    );
    assert!(matches!(
        parse("{{< inner >}}text"),
        Err(ParseError::Unclosed { name, .. }) if name == "inner"
    ));

    // A closing tag (or `/>`) for a shortcode that does not use `inner` is an error.
    assert!(matches!(
        parse("{{< plain >}}x{{< /plain >}}"),
        Err(ParseError::UnexpectedClose { name, span }) if name == "plain" && span == (14..28)
    ));
    assert!(matches!(
        parse("{{< plain />}}"),
        Err(ParseError::ClosingNotAllowed { name, .. }) if name == "plain"
    ));

    assert!(matches!(
        parse("{{< nope >}}"),
        Err(ParseError::UnknownShortcode { name, span }) if name == "nope" && span == (0..8)
    ));
}

#[test]
fn malformed_calls() {
    assert!(matches!(parse("{{< >}}"), Err(ParseError::NoName { span }) if span == (0..7)));
    assert!(matches!(
        parse("{{< outer >}}{{< inner >}}{{< /outer >}}"),
        Err(ParseError::MismatchedClose { name, found, .. }) if name == "inner" && found == "outer"
    ));
    assert!(matches!(parse("{{< plain"), Err(ParseError::Lex(_))));
}

#[test]
fn escaped_shortcodes_are_text() {
    let body = parse("a {{</* plain x */>}} b").unwrap();
    assert_eq!(
        body.segments,
        vec![
            Segment::Text("a {{<"),
            Segment::Text(" plain x "),
            Segment::Text(">}} b"),
        ]
    );
}

#[test]
fn summary_divider() {
    let body = parse("Sum {{< plain >}}<!--more-->\nRest <!--more--> x").unwrap();
    assert_eq!(body.summary_divider, Some(2));
    assert_eq!(body.segments[2], Segment::Text("Rest <!--more--> x"));

    // Inside inner content the divider is dropped (as in Go).
    let body = parse("{{< outer >}}a<!--more-->b{{< /outer >}}").unwrap();
    assert_eq!(body.summary_divider, None);
    let Closing::Closed { inner, .. } = &call(&body.segments[0]).closing else {
        panic!()
    };
    assert_eq!(inner, &[Segment::Text("a"), Segment::Text("b")]);
}

#[test]
fn inline_shortcodes() {
    let src = "{{< time.inline >}}{{ now }}{{< /time.inline >}} {{< time.inline />}}";
    let body = parse(src).unwrap();
    let c = call(&body.segments[0]);
    assert!(c.inline);
    assert_eq!(c.name, "time.inline");
    assert!(
        matches!(&c.closing, Closing::Closed { inner, .. } if inner == &[Segment::Text("{{ now }}")])
    );
    assert_eq!(call(&body.segments[2]).closing, Closing::SelfClosed);
}

/// Every content file of the oracle (docs, testsite, skeletons) whose body
/// lexes assembles, with an oracle that says a shortcode uses `inner` when the file closes or
/// self-closes it; the calls' spans nest and the text reassembles the source.
#[test]
fn content_files_assemble() {
    let mut tally = Tally::new("pageparser/assemble content files");
    let mut calls = 0;
    for (id, c) in page_cases() {
        if !id.starts_with("file:") {
            continue;
        }
        let Some(src) = c["src"].as_str() else {
            continue;
        };
        let Ok(split) = split_front_matter(src) else {
            continue;
        };
        let Ok(tokens) = lex(split.body) else {
            tally.skipped += 1;
            continue;
        };
        let body = split.body;
        let mut closed = BTreeSet::new();
        for (i, t) in tokens.iter().enumerate() {
            if t.kind != TokenKind::Close {
                continue;
            }
            // The name after the `/` of a closing tag, or the name of a self-closing tag.
            let is_name = |t: &&ssg_pageparser::Token| {
                matches!(t.kind, TokenKind::Name | TokenKind::InlineName)
            };
            let name = tokens
                .get(i + 1)
                .filter(is_name)
                .or_else(|| tokens[..i].iter().rev().find(is_name));
            if let Some(n) = name {
                closed.insert(n.text(body));
            }
        }
        let oracle = |name: &str| {
            if closed.contains(name) {
                InnerUse::Required
            } else {
                InnerUse::Unused
            }
        };
        let res = assemble(body, &tokens, &oracle);
        tally.check(res.is_ok(), || format!("{id}: {res:?}"));
        if let Ok(b) = res {
            let mut covered = 0;
            check_segments(&b.segments, body, &mut covered, &mut calls);
        }
    }
    eprintln!("pageparser/assemble: {calls} shortcode calls");
    assert!(calls > 500, "{calls}");
    tally.finish();
}

/// Segments are in source order and inside the source; nested calls inside their parent.
fn check_segments(segs: &[Segment<'_>], src: &str, pos: &mut usize, calls: &mut usize) {
    for s in segs {
        match s {
            Segment::Text(t) => {
                let start = t.as_ptr() as usize - src.as_ptr() as usize;
                assert!(start >= *pos, "text out of order");
                *pos = start + t.len();
            }
            Segment::Shortcode(c) => {
                *calls += 1;
                assert!(c.span.start >= *pos, "call out of order");
                if let Closing::Closed { inner, span } = &c.closing {
                    let mut inner_pos = span.start;
                    check_segments(inner, src, &mut inner_pos, calls);
                    assert!(inner_pos <= span.end && span.end <= c.span.end);
                }
                *pos = c.span.end;
            }
        }
    }
}
