//! Chroma's `wdte.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "wdte",
    config: ConfigDef {
        name: "WDTE",
        filenames: &["*.wdte"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"#(.*?)\n").token(T::CommentSingle),
            rule(r"-?[0-9]+").token(T::LiteralNumberInteger),
            rule(r"-?[0-9]*\.[0-9]+").token(T::LiteralNumberFloat),
            rule(r#""[^"]*""#).token(T::LiteralString),
            rule(r"'[^']*'").token(T::LiteralString),
            rule(r"(default|switch|memo)\b").token(T::KeywordReserved),
            rule(r"{|}|;|->|=>|\(|\)|\[|\]|\.").token(T::Operator),
            rule(r"[^{};()[\].\s]+").token(T::NameVariable),
        ]),
    ],
};
