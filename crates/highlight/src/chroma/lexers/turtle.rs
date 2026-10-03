//! Chroma's `turtle.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "turtle",
    config: ConfigDef {
        name: "Turtle",
        aliases: &["turtle"],
        filenames: &["*.ttl"],
        mime_types: &["text/turtle", "application/x-turtle"],
        case_insensitive: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("triple-double-quoted-string", &[
            rule(r#"""""#).token(T::LiteralString).push(&["end-of-string"]),
            rule(r"[^\\]+").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString).push(&["string-escape"]),
        ]),
        ("single-double-quoted-string", &[
            rule(r#"""#).token(T::LiteralString).push(&["end-of-string"]),
            rule(r#"[^"\\\n]+"#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralString).push(&["string-escape"]),
        ]),
        ("triple-single-quoted-string", &[
            rule(r"'''").token(T::LiteralString).push(&["end-of-string"]),
            rule(r"[^\\]+").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString).push(&["string-escape"]),
        ]),
        ("single-single-quoted-string", &[
            rule(r"'").token(T::LiteralString).push(&["end-of-string"]),
            rule(r"[^'\\\n]+").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString).push(&["string-escape"]),
        ]),
        ("string-escape", &[
            rule(r".").token(T::LiteralString).pop(1),
        ]),
        ("end-of-string", &[
            rule(r"(@)([a-z]+(:?-[a-z0-9]+)*)").groups(&[T::Operator, T::GenericEmph, T::GenericEmph]).pop(2),
            rule(r#"(\^\^)(<[^<>"{}|^`\\\x00-\x20]*>)"#).groups(&[T::Operator, T::GenericEmph]).pop(2),
            rule(r"(\^\^)((?:[a-z][\w-]*)?\:)([a-z][\w-]*)").groups(&[T::Operator, T::GenericEmph, T::GenericEmph]).pop(2),
            rule("").pop(2),
        ]),
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r#"(@base|BASE)(\s+)(<[^<>"{}|^`\\\x00-\x20]*>)(\s*)(\.?)"#).groups(&[T::Keyword, T::TextWhitespace, T::NameVariable, T::TextWhitespace, T::Punctuation]),
            rule(r#"(@prefix|PREFIX)(\s+)((?:[a-z][\w-]*)?\:)(\s+)(<[^<>"{}|^`\\\x00-\x20]*>)(\s*)(\.?)"#).groups(&[T::Keyword, T::TextWhitespace, T::NameNamespace, T::TextWhitespace, T::NameVariable, T::TextWhitespace, T::Punctuation]),
            rule(r"(?<=\s)a(?=\s)").token(T::KeywordType),
            rule(r#"(<[^<>"{}|^`\\\x00-\x20]*>)"#).token(T::NameVariable),
            rule(r"((?:[a-z][\w-]*)?\:)([a-z][\w-]*)").groups(&[T::NameNamespace, T::NameTag]),
            rule(r"#[^\n]+").token(T::Comment),
            rule(r"\b(true|false)\b").token(T::Literal),
            rule(r"[+\-]?\d*\.\d+").token(T::LiteralNumberFloat),
            rule(r"[+\-]?\d*(:?\.\d+)?E[+\-]?\d+").token(T::LiteralNumberFloat),
            rule(r"[+\-]?\d+").token(T::LiteralNumberInteger),
            rule(r"[\[\](){}.;,:^]").token(T::Punctuation),
            rule(r#"""""#).token(T::LiteralString).push(&["triple-double-quoted-string"]),
            rule(r#"""#).token(T::LiteralString).push(&["single-double-quoted-string"]),
            rule(r"'''").token(T::LiteralString).push(&["triple-single-quoted-string"]),
            rule(r"'").token(T::LiteralString).push(&["single-single-quoted-string"]),
        ]),
    ],
};
