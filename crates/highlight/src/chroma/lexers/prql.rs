//! Chroma's `prql.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "prql",
    config: ConfigDef {
        name: "PRQL",
        aliases: &["prql"],
        filenames: &["*.prql"],
        mime_types: &["application/prql"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"#!.*").token(T::LiteralStringDoc),
            rule(r"#.*").token(T::CommentSingle),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"^(\s*)(module)(\s*)").groups(&[T::TextWhitespace, T::KeywordNamespace, T::TextWhitespace]).push(&["imports"]),
            rule(r"(bool|int|int8|int16|int32|int64|int128|float|text|set)\b").token(T::KeywordType),
            rule(r"^prql ").token(T::KeywordReserved),
            rule(r"let").token(T::KeywordDeclaration),
            include("keywords"),
            include("expr"),
            rule(r"^[A-Za-z_][a-zA-Z0-9_]*").token(T::Keyword),
        ]),
        ("expr", &[
            rule(r#"(f)(""")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).combined(&["fstringescape", "tdqf"]),
            rule(r"(f)(''')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).combined(&["fstringescape", "tsqf"]),
            rule(r#"(f)(")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).combined(&["fstringescape", "dqf"]),
            rule(r"(f)(')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).combined(&["fstringescape", "sqf"]),
            rule(r#"(s)(""")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).combined(&["stringescape", "tdqf"]),
            rule(r"(s)(''')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).combined(&["stringescape", "tsqf"]),
            rule(r#"(s)(")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).combined(&["stringescape", "dqf"]),
            rule(r"(s)(')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).combined(&["stringescape", "sqf"]),
            rule(r#"(?i)(r)(""")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).push(&["tdqs"]),
            rule(r"(?i)(r)(''')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).push(&["tsqs"]),
            rule(r#"(?i)(r)(")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).push(&["dqs"]),
            rule(r"(?i)(r)(')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).push(&["sqs"]),
            rule(r#"""""#).token(T::LiteralStringDouble).combined(&["stringescape", "tdqs"]),
            rule(r"'''").token(T::LiteralStringSingle).combined(&["stringescape", "tsqs"]),
            rule(r#"""#).token(T::LiteralStringDouble).combined(&["stringescape", "dqs"]),
            rule(r"'").token(T::LiteralStringSingle).combined(&["stringescape", "sqs"]),
            rule(r"@\d{4}-\d{2}-\d{2}T\d{2}(:\d{2})?(:\d{2})?(\.\d{1,6})?(Z|[+-]\d{1,2}(:\d{1,2})?)?").token(T::LiteralDate),
            rule(r"@\d{4}-\d{2}-\d{2}").token(T::LiteralDate),
            rule(r"@\d{2}(:\d{2})?(:\d{2})?(\.\d{1,6})?(Z|[+-]\d{1,2}(:\d{1,2})?)?").token(T::LiteralDate),
            rule(r"[^\S\n]+").token(T::Text),
            include("numbers"),
            rule(r"->|=>|==|!=|>=|<=|~=|&&|\|\||\?\?|\/\/").token(T::Operator),
            rule(r"[-~+/*%=<>&^|.@]").token(T::Operator),
            rule(r"[]{}:(),;[]").token(T::Punctuation),
            include("functions"),
            rule(r"[A-Za-z_][a-zA-Z0-9_]*").token(T::NameVariable),
        ]),
        ("numbers", &[
            rule(r"(\d(?:_?\d)*\.(?:\d(?:_?\d)*)?|(?:\d(?:_?\d)*)?\.\d(?:_?\d)*)([eE][+-]?\d(?:_?\d)*)?").token(T::LiteralNumberFloat),
            rule(r"\d(?:_?\d)*[eE][+-]?\d(?:_?\d)*j?").token(T::LiteralNumberFloat),
            rule(r"0[oO](?:_?[0-7])+").token(T::LiteralNumberOct),
            rule(r"0[bB](?:_?[01])+").token(T::LiteralNumberBin),
            rule(r"0[xX](?:_?[a-fA-F0-9])+").token(T::LiteralNumberHex),
            rule(r"\d(?:_?\d)*").token(T::LiteralNumberInteger),
        ]),
        ("fstringescape", &[
            include("stringescape"),
        ]),
        ("bytesescape", &[
            rule(r#"\\([\\bfnrt"\']|\n|x[a-fA-F0-9]{2}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
        ]),
        ("stringescape", &[
            rule(r"\\(N\{.*?\}|u\{[a-fA-F0-9]{1,6}\})").token(T::LiteralStringEscape),
            include("bytesescape"),
        ]),
        ("fstrings-single", &[
            rule(r"\}").token(T::LiteralStringInterpol),
            rule(r"\{").token(T::LiteralStringInterpol).push(&["expr-inside-fstring"]),
            rule(r#"[^\\\'"{}\n]+"#).token(T::LiteralStringSingle),
            rule(r#"[\'"\\]"#).token(T::LiteralStringSingle),
        ]),
        ("fstrings-double", &[
            rule(r"\}").token(T::LiteralStringInterpol),
            rule(r"\{").token(T::LiteralStringInterpol).push(&["expr-inside-fstring"]),
            rule(r#"[^\\\'"{}\n]+"#).token(T::LiteralStringDouble),
            rule(r#"[\'"\\]"#).token(T::LiteralStringDouble),
        ]),
        ("strings-single", &[
            rule(r"\{((\w+)((\.\w+)|(\[[^\]]+\]))*)?(\:(.?[<>=\^])?[-+ ]?#?0?(\d+)?,?(\.\d+)?[E-GXb-gnosx%]?)?\}").token(T::LiteralStringInterpol),
            rule(r#"[^\\\'"%{\n]+"#).token(T::LiteralStringSingle),
            rule(r#"[\'"\\]"#).token(T::LiteralStringSingle),
            rule(r"%|(\{{1,2})").token(T::LiteralStringSingle),
        ]),
        ("strings-double", &[
            rule(r"\{((\w+)((\.\w+)|(\[[^\]]+\]))*)?(\:(.?[<>=\^])?[-+ ]?#?0?(\d+)?,?(\.\d+)?[E-GXb-gnosx%]?)?\}").token(T::LiteralStringInterpol),
            rule(r#"[^\\\'"%{\n]+"#).token(T::LiteralStringDouble),
            rule(r#"[\'"\\]"#).token(T::LiteralStringDouble),
            rule(r"%|(\{{1,2})").token(T::LiteralStringDouble),
        ]),
        ("dqf", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"\\\\|\\"|\\\n"#).token(T::LiteralStringEscape),
            include("fstrings-double"),
        ]),
        ("sqf", &[
            rule(r"'").token(T::LiteralStringSingle).pop(1),
            rule(r"\\\\|\\'|\\\n").token(T::LiteralStringEscape),
            include("fstrings-single"),
        ]),
        ("dqs", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"\\\\|\\"|\\\n"#).token(T::LiteralStringEscape),
            include("strings-double"),
        ]),
        ("sqs", &[
            rule(r"'").token(T::LiteralStringSingle).pop(1),
            rule(r"\\\\|\\'|\\\n").token(T::LiteralStringEscape),
            include("strings-single"),
        ]),
        ("tdqf", &[
            rule(r#"""""#).token(T::LiteralStringDouble).pop(1),
            include("fstrings-double"),
            rule(r"\n").token(T::LiteralStringDouble),
        ]),
        ("tsqf", &[
            rule(r"'''").token(T::LiteralStringSingle).pop(1),
            include("fstrings-single"),
            rule(r"\n").token(T::LiteralStringSingle),
        ]),
        ("tdqs", &[
            rule(r#"""""#).token(T::LiteralStringDouble).pop(1),
            include("strings-double"),
            rule(r"\n").token(T::LiteralStringDouble),
        ]),
        ("tsqs", &[
            rule(r"'''").token(T::LiteralStringSingle).pop(1),
            include("strings-single"),
            rule(r"\n").token(T::LiteralStringSingle),
        ]),
        ("expr-inside-fstring", &[
            rule(r"[{([]").token(T::Punctuation).push(&["expr-inside-fstring-inner"]),
            rule(r"(=\s*)?\}").token(T::LiteralStringInterpol).pop(1),
            rule(r"(=\s*)?:").token(T::LiteralStringInterpol).pop(1),
            rule(r"\s+").token(T::TextWhitespace),
            include("expr"),
        ]),
        ("expr-inside-fstring-inner", &[
            rule(r"[{([]").token(T::Punctuation).push(&["expr-inside-fstring-inner"]),
            rule(r"[])}]").token(T::Punctuation).pop(1),
            rule(r"\s+").token(T::TextWhitespace),
            include("expr"),
        ]),
        ("keywords", &[
            rule(r"(into|case|type|module|internal)\b").token(T::Keyword),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
        ]),
        ("functions", &[
            rule(r"(min|max|sum|average|stddev|every|any|concat_array|count|lag|lead|first|last|rank|rank_dense|row_number|round|as|in|tuple_every|tuple_map|tuple_zip|_eq|_is_null|from_text|lower|upper|read_parquet|read_csv)\b").token(T::NameFunction),
        ]),
        ("comment", &[
            rule(r"-(?!\})").token(T::CommentMultiline),
            rule(r"\{-").token(T::CommentMultiline).push(&["comment"]),
            rule(r"[^-}]").token(T::CommentMultiline),
            rule(r"-\}").token(T::CommentMultiline).pop(1),
        ]),
        ("imports", &[
            rule(r"\w+(\.\w+)*").token(T::NameClass).pop(1),
        ]),
    ],
};
