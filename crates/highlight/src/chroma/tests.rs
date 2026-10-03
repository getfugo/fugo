//! The engine against Chroma: every lexer compiles, lookup follows Chroma's table.

use super::*;

fn registry() -> Registry {
    golexers::registry()
}

#[test]
fn every_lexer_compiles() {
    let reg = registry();
    assert_eq!(reg.lexers().len(), 270);
    for l in reg.lexers() {
        // Tokenising compiles the rules; an empty text exercises nothing else.
        let tokens = l.tokenise(&reg, None, "");
        assert!(
            tokens.iter().all(|t| t.ty != TokenType::Error),
            "{}",
            l.config().name
        );
    }
    for def in lexers::LEXERS.iter().chain(golexers::exported::LEXERS) {
        let lexer = RegexLexer::from_table(def);
        assert_eq!(lexer.compile_error(), None, "{}", def.file);
    }
}

#[test]
fn lookup_follows_chromas_table() {
    let reg = registry();
    let table = include_str!("../data/chroma-lexers.tsv");
    let mut failures = Vec::new();
    for line in table.lines().filter(|l| !l.starts_with('#')) {
        let mut cols = line.split('\t');
        let (Some(_), Some(key), Some(want)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let got = reg.get(key).map(|l| l.config().name.clone());
        if got.as_deref() != Some(want) {
            failures.push(format!("{key:?}: want {want:?}, got {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(reg.get("no-such-language").is_none());
}

#[test]
fn coalesce_and_ensure_lf() {
    assert_eq!(ensure_lf("a\r\nb\rc"), "a\nb\nc");
    let t = |ty, v: &str| Token::new(ty, v);
    let out = coalesce(vec![
        t(TokenType::Text, "a"),
        t(TokenType::Text, ""),
        t(TokenType::Text, "b"),
        t(TokenType::EOFType, ""),
        t(TokenType::Text, "c"),
        t(TokenType::Name, "d"),
    ]);
    assert_eq!(
        out,
        vec![
            t(TokenType::Text, "ab"),
            t(TokenType::Text, "c"),
            t(TokenType::Name, "d")
        ]
    );
    // With no run pending, `EOF` ends the stream.
    let out = coalesce(vec![
        t(TokenType::Text, "a"),
        t(TokenType::EOFType, ""),
        t(TokenType::EOFType, ""),
        t(TokenType::Text, "b"),
    ]);
    assert_eq!(out, vec![t(TokenType::Text, "a")]);
}

/// Coalesced tokens of `code` in `lang`, as `type:text` strings.
fn tokens(reg: &Registry, lang: &str, code: &str) -> Vec<String> {
    let lexer = reg.get(lang).expect(lang);
    coalesce(lexer.tokenise(reg, None, code))
        .into_iter()
        .map(|t| format!("{}:{}", t.ty.name(), t.value))
        .collect()
}

/// Zero-width matches that bring the lexer back to where it was (Chroma loops forever there)
/// end: the position is treated as matched by no rule.
#[test]
fn zero_width_cycles_end() {
    let reg = registry();
    // JSONata's last rule `[a-zA-Z0-9_]*` matches the empty string anywhere.
    assert_eq!(
        tokens(&reg, "jsonata", "a é b\n"),
        ["Name:a", "Text: ", "Error:é", "Text: ", "Name:b", "Text:\n"]
    );
    // Jungle's `(?=\S)` pushes `var`, whose default pops back to `instruction`.
    assert_eq!(
        tokens(&reg, "jungle", "\"x\n"),
        ["Error:\"", "Name:x", "Text:\n"]
    );
}

/// A zero-width push of the current state grows the stack without bound (Chroma runs out of
/// memory): past `MAX_STALL` configurations the position is matched by no rule.
#[test]
fn zero_width_stack_growth_ends() {
    fn rules() -> Rules {
        let rule = |pattern: &str, emitter, mutator| Rule {
            pattern: pattern.to_owned(),
            emitter,
            mutator,
        };
        Rules::from([(
            "root".to_owned(),
            vec![
                rule("b", Some(Emitter::Token(TokenType::Name)), None),
                rule("", None, Some(regex_lexer::Mutator::Push(Vec::new()))),
            ],
        )])
    }
    let reg = registry();
    let lexer = RegexLexer::from_code(Config::default(), rules);
    let got: Vec<String> = coalesce(lexer.tokenise(&reg, None, "ab\na"))
        .into_iter()
        .map(|t| format!("{}:{}", t.ty.name(), t.value))
        .collect();
    assert_eq!(got, ["Error:a", "Name:b", "Error:\na"]);
}

/// Deep state stacks with zero-width matches at every position, and Haxe's pre-processor
/// saving the stack at every `#if`, tokenise in linear time: the guard of the previous tests
/// and the pre-processor copy stacks in O(1). With copies of the whole stack, Haxe's
/// `(`×12,000 `)`×12,000 took 28 s in a release build (Chroma: 65 ms). Same tokens as Chroma
/// v2.19.0 (Go oracle).
#[test]
fn deep_stacks_tokenise_in_linear_time() {
    let reg = registry();
    let n = 20_000;
    let code = format!("{}{}\n", "(".repeat(n), ")".repeat(n));
    let lexer = reg.get("haxe").expect("haxe");
    let got = coalesce(lexer.tokenise(&reg, None, &code));
    assert_eq!(
        got.iter().map(|t| t.value.as_str()).collect::<String>(),
        code
    );
    for t in &got {
        let want = match t.value.chars().next() {
            Some('(') => TokenType::Punctuation,
            Some(')') => TokenType::Error,
            _ => TokenType::Text,
        };
        assert_eq!(t.ty, want, "{:?}", t.value.get(..8));
    }
    let n = 2_000;
    let ifs: String = (0..n)
        .map(|i| format!("#if a{i}\nvar x{i} = (1;\n#else\n"))
        .collect();
    let code = format!("class C {{\n{ifs}{}}}\n", "#end\n".repeat(n));
    let got = tokens(&reg, "haxe", &code);
    assert_eq!(got.len(), 6 + 12 * n + 2 * n + 2);
    assert_eq!(
        got[6..18],
        [
            "CommentPreproc:#if a0",
            "Text:\n",
            "KeywordDeclaration:var",
            "Text: x0 ",
            "Operator:=",
            "Text: ",
            "Punctuation:(",
            "LiteralNumberInteger:1",
            "Error:;",
            "Text:\n",
            "CommentPreproc:#else",
            "Text:\n",
        ]
    );
    assert_eq!(got[got.len() - 2..], ["Punctuation:}", "Text:\n"]);
}

/// An HTTP message inside another language ends where its body starts: Chroma's consumers of
/// a nested iterator stop at its `EOF` (Go oracle, Chroma v2.19.0).
#[test]
fn nested_http_ends_at_its_body() {
    let reg = registry();
    let md = "# Title\n\n```http\nPOST /api HTTP/1.1\nContent-Type: application/json\n\n\
              {\"a\": [1]}\n```\n\nafter *em* text\n";
    assert_eq!(
        tokens(&reg, "markdown", md),
        [
            "GenericHeading:# Title\n",
            "Text:\n",
            "LiteralString:```http\n",
            "NameFunction:POST",
            "Text: ",
            "NameNamespace:/api",
            "Text: ",
            "KeywordReserved:HTTP",
            "Operator:/",
            "LiteralNumber:1.1",
            "Text:\n",
            "Name:Content-Type",
            "Operator::",
            "Text: ",
            "Literal:application/json",
            "Text:\n\n",
            "LiteralString:```",
            "Text:\n\nafter ",
            "GenericEmph:*em*",
            "Text: text\n",
        ]
    );
    // At the top level the body follows, as its own lexer's tokens.
    let http = "POST /api HTTP/1.1\nContent-Type: application/json\n\n{\"a\": 1}\n";
    assert_eq!(
        tokens(&reg, "http", http)[11..],
        [
            "Literal:application/json",
            "Text:\n\n",
            "Punctuation:{",
            "NameTag:\"a\"",
            "Punctuation::",
            "Text: ",
            "LiteralNumberInteger:1",
            "Punctuation:}",
            "Text:\n",
        ]
    );
}

/// DNS's analyser: `(?m)^@\s+IN\s+SOA\s+` in Go's RE2 (Go oracle, Chroma v2.19.0).
#[test]
fn dns_analyser_spans_lines() {
    let reg = registry();
    let dns = reg.get("dns").expect("dns");
    for (text, score) in [
        ("$ORIGIN example.com.\n@\nIN SOA ns1 admin 1 2 3 4 5\n", 1.0),
        ("$ORIGIN example.com.\n@ IN SOA ns1 admin 1 2 3 4 5\n", 1.0),
        ("x\n@ \n\tIN\n\nSOA\n", 1.0),
        ("x\n@ IN SOA", 0.0),
        (" @ IN SOA x", 0.0),
        ("@\u{b}IN SOA x", 0.0),
    ] {
        assert_eq!(dns.analyse_text(text), score, "{text:?}");
    }
}
