//! Chroma's `dylan.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "dylan",
    config: ConfigDef {
        name: "Dylan",
        aliases: &["dylan"],
        filenames: &["*.dylan", "*.dyl", "*.intr"],
        mime_types: &["text/x-dylan"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-f0-9]{2,4}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"([a-z0-9-]+:)([ \t]*)(.*(?:\n[ \t].+)*)").groups(&[T::NameAttribute, T::TextWhitespace, T::LiteralString]),
            rule("").push(&["code"]),
        ]),
        ("code", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*").token(T::CommentMultiline).push(&["comment"]),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"'(\\.|\\[0-7]{1,3}|\\x[a-f0-9]{1,2}|[^\\\'\n])'").token(T::LiteralStringChar),
            rule(r"#b[01]+").token(T::LiteralNumberBin),
            rule(r"#o[0-7]+").token(T::LiteralNumberOct),
            rule(r"[-+]?(\d*\.\d+([ed][-+]?\d+)?|\d+(\.\d*)?e[-+]?\d+)").token(T::LiteralNumberFloat),
            rule(r"[-+]?\d+").token(T::LiteralNumberInteger),
            rule(r"#x[0-9a-f]+").token(T::LiteralNumberHex),
            rule(r"(\?\\?)([\w!&*<>|^$%@+~?/=-]+)(:)(token|name|variable|expression|body|case-body|\*)").groups(&[T::Operator, T::NameVariable, T::Operator, T::NameBuiltin]),
            rule(r"(\?)(:)(token|name|variable|expression|body|case-body|\*)").groups(&[T::Operator, T::Operator, T::NameVariable]),
            rule(r"(\?\\?)([\w!&*<>|^$%@+~?/=-]+)").groups(&[T::Operator, T::NameVariable]),
            rule(r"(=>|::|#\(|#\[|##|\?\?|\?=|\?|[(){}\[\],.;])").token(T::Punctuation),
            rule(r":=").token(T::Operator),
            rule(r"#[tf]").token(T::Literal),
            rule(r#"#""#).token(T::LiteralStringSymbol).push(&["symbol"]),
            rule(r"#[a-z0-9-]+").token(T::Keyword),
            rule(r"#(all-keys|include|key|next|rest)").token(T::Keyword),
            rule(r"[\w!&*<>|^$%@+~?/=-]+:").token(T::KeywordConstant),
            rule(r"<[\w!&*<>|^$%@+~?/=-]+>").token(T::NameClass),
            rule(r"\*[\w!&*<>|^$%@+~?/=-]+\*").token(T::NameVariableGlobal),
            rule(r"\$[\w!&*<>|^$%@+~?/=-]+").token(T::NameConstant),
            rule(r"(let|method|function)([ \t]+)([\w!&*<>|^$%@+~?/=-]+)").groups(&[T::NameBuiltin, T::TextWhitespace, T::NameVariable]),
            rule(r"(error|signal|return|break)").token(T::NameException),
            rule(r"(\\?)([\w!&*<>|^$%@+~?/=-]+)").groups(&[T::Operator, T::Name]),
        ]),
        ("comment", &[
            rule(r"[^*/]").token(T::CommentMultiline),
            rule(r"/\*").token(T::CommentMultiline).push(&[]),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"[*/]").token(T::CommentMultiline),
        ]),
        ("symbol", &[
            rule(r#"""#).token(T::LiteralStringSymbol).pop(1),
            rule(r#"[^\\"]+"#).token(T::LiteralStringSymbol),
        ]),
    ],
};
