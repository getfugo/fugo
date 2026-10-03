//! Chroma's `groff.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "groff",
    config: ConfigDef {
        name: "Groff",
        aliases: &["groff", "nroff", "man"],
        filenames: &["*.[1-9]", "*.1p", "*.3pm", "*.man"],
        mime_types: &["application/x-troff", "text/troff"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("request", &[
            rule(r"\n").token(T::Text).pop(1),
            include("escapes"),
            rule(r#""[^\n"]+""#).token(T::LiteralStringDouble),
            rule(r"\d+").token(T::LiteralNumber),
            rule(r"\S+").token(T::LiteralString),
            rule(r"\s+").token(T::Text),
        ]),
        ("root", &[
            rule(r"(\.)(\w+)").groups(&[T::Text, T::Keyword]).push(&["request"]),
            rule(r"\.").token(T::Punctuation).push(&["request"]),
            rule(r"[^\\\n]+").token(T::Text).push(&["textline"]),
            rule("").push(&["textline"]),
        ]),
        ("textline", &[
            include("escapes"),
            rule(r"[^\\\n]+").token(T::Text),
            rule(r"\n").token(T::Text).pop(1),
        ]),
        ("escapes", &[
            rule(r#"\\"[^\n]*"#).token(T::Comment),
            rule(r"\\[fn]\w").token(T::LiteralStringEscape),
            rule(r"\\\(.{2}").token(T::LiteralStringEscape),
            rule(r"\\.\[.*\]").token(T::LiteralStringEscape),
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r"\\\n").token(T::Text).push(&["request"]),
        ]),
    ],
};
