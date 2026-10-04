use super::*;

fn kinds(src: &str) -> Vec<(TokenKind, &str)> {
    lex(src)
        .unwrap()
        .into_iter()
        .map(|t| (t.kind, &src[t.span]))
        .collect()
}

#[test]
fn text_splits_off_indentation() {
    use TokenKind::*;
    assert_eq!(
        kinds("a\n  {{< x >}}"),
        vec![
            (Text, "a\n"),
            (Indentation, "  "),
            (LeftDelim(Delim::Html), "{{<"),
            (Name, "x"),
            (RightDelim(Delim::Html), ">}}"),
        ]
    );
    assert_eq!(kinds("  "), vec![(Indentation, "  ")]);
    assert_eq!(kinds("a  "), vec![(Text, "a  ")]);
    assert_eq!(kinds("a\n"), vec![(Text, "a\n")]);
}

#[test]
fn arguments() {
    use TokenKind::*;
    let src = r#"{{% x a="b \"c\"" n=1 r=`raw` %}}"#;
    let toks = lex(src).unwrap();
    let got: Vec<_> = toks.iter().map(|t| (t.kind, t.value(src))).collect();
    assert_eq!(
        got,
        vec![
            (LeftDelim(Delim::Markdown), "{{%".into()),
            (Name, "x".into()),
            (Param(Quoting::Bare), "a".into()),
            (Value(Quoting::Escaped), "b \"c\"".into()),
            (Param(Quoting::Bare), "n".into()),
            (Value(Quoting::Bare), "1".into()),
            (Param(Quoting::Bare), "r".into()),
            (Value(Quoting::Backtick), "raw".into()),
            (RightDelim(Delim::Markdown), "%}}".into()),
        ]
    );
}

#[test]
fn errors() {
    let e = lex("{{< x a b=c >}}").unwrap_err();
    assert_eq!(e.kind, LexErrorKind::MixedArguments);
    let e = lex("{{< /x >}}").unwrap_err();
    assert_eq!(e.kind, LexErrorKind::CloseWithoutOpen);
    let e = lex("{{< x").unwrap_err();
    assert_eq!(e.kind, LexErrorKind::UnclosedTag);
}

#[test]
fn decoding_matches_go() {
    assert_eq!(decode(b"\xff"), Some(('\u{fffd}', 1)));
    assert_eq!(decode(b"\xc3"), Some(('\u{fffd}', 1)));
    assert_eq!(decode("é".as_bytes()), Some(('é', 2)));
    assert_eq!(decode_last(b"a\xc3"), Some(('\u{fffd}', 1)));
    assert_eq!(decode_last("aé".as_bytes()), Some(('é', 2)));
}
