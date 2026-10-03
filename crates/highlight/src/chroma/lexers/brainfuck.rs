//! Chroma's `brainfuck.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "brainfuck",
    config: ConfigDef {
        name: "Brainfuck",
        aliases: &["brainfuck", "bf"],
        filenames: &["*.bf", "*.b"],
        mime_types: &["application/x-brainfuck"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("common", &[
            rule(r"[.,]+").token(T::NameTag),
            rule(r"[+-]+").token(T::NameBuiltin),
            rule(r"[<>]+").token(T::NameVariable),
            rule(r"[^.,+\-<>\[\]]+").token(T::Comment),
        ]),
        ("root", &[
            rule(r"\[").token(T::Keyword).push(&["loop"]),
            rule(r"\]").token(T::Error),
            include("common"),
        ]),
        ("loop", &[
            rule(r"\[").token(T::Keyword).push(&[]),
            rule(r"\]").token(T::Keyword).pop(1),
            include("common"),
        ]),
    ],
};
