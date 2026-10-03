//! Chroma's `json.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "json",
    config: ConfigDef {
        name: "JSON",
        aliases: &["json"],
        filenames: &["*.json", "*.jsonc", "*.avsc"],
        mime_types: &["application/json"],
        dot_all: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            include("value"),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::Text),
        ]),
        ("comment", &[
            rule(r"//.*?\n").token(T::CommentSingle),
        ]),
        ("simplevalue", &[
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"-?(0|[1-9]\d*)(\.\d+[eE](\+|-)?\d+|[eE](\+|-)?\d+|\.\d+)").token(T::LiteralNumberFloat),
            rule(r"-?(0|[1-9]\d*)").token(T::LiteralNumberInteger),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
        ]),
        ("objectattribute", &[
            include("value"),
            rule(r":").token(T::Punctuation),
            rule(r",").token(T::Punctuation).pop(1),
            rule(r"\}").token(T::Punctuation).pop(2),
        ]),
        ("objectvalue", &[
            include("whitespace"),
            include("comment"),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::NameTag).push(&["objectattribute"]),
            rule(r"\}").token(T::Punctuation).pop(1),
        ]),
        ("arrayvalue", &[
            include("whitespace"),
            include("value"),
            include("comment"),
            rule(r",").token(T::Punctuation),
            rule(r"\]").token(T::Punctuation).pop(1),
        ]),
        ("value", &[
            include("whitespace"),
            include("simplevalue"),
            include("comment"),
            rule(r"\{").token(T::Punctuation).push(&["objectvalue"]),
            rule(r"\[").token(T::Punctuation).push(&["arrayvalue"]),
        ]),
    ],
};
