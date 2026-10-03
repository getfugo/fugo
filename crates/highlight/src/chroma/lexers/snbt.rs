//! Chroma's `snbt.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "snbt",
    config: ConfigDef {
        name: "SNBT",
        aliases: &["snbt"],
        filenames: &["*.snbt"],
        mime_types: &["text/snbt"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\{").token(T::Punctuation).push(&["compound"]),
            rule(r"[^\{]+").token(T::Text),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::TextWhitespace),
        ]),
        ("operators", &[
            rule(r"[,:;]").token(T::Punctuation),
        ]),
        ("literals", &[
            rule(r"(true|false)").token(T::KeywordConstant),
            rule(r"-?\d+[eE]-?\d+").token(T::LiteralNumberFloat),
            rule(r"-?\d*\.\d+[fFdD]?").token(T::LiteralNumberFloat),
            rule(r"-?\d+[bBsSlLfFdD]?").token(T::LiteralNumberInteger),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["literals.string_double"]),
            rule(r"'").token(T::LiteralStringSingle).push(&["literals.string_single"]),
        ]),
        ("literals.string_double", &[
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
        ("literals.string_single", &[
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r"[^\\'\n]+").token(T::LiteralStringSingle),
            rule(r"'").token(T::LiteralStringSingle).pop(1),
        ]),
        ("compound", &[
            rule(r"[A-Z_a-z]+").token(T::NameAttribute),
            include("operators"),
            include("whitespace"),
            include("literals"),
            rule(r"\{").token(T::Punctuation).push(&[]),
            rule(r"\[").token(T::Punctuation).push(&["list"]),
            rule(r"\}").token(T::Punctuation).pop(1),
        ]),
        ("list", &[
            rule(r"[A-Z_a-z]+").token(T::NameAttribute),
            include("literals"),
            include("operators"),
            include("whitespace"),
            rule(r"\[").token(T::Punctuation).push(&[]),
            rule(r"\{").token(T::Punctuation).push(&["compound"]),
            rule(r"\]").token(T::Punctuation).pop(1),
        ]),
    ],
};
