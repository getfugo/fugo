//! Chroma's `terminfo.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "terminfo",
    config: ConfigDef {
        name: "Terminfo",
        aliases: &["terminfo"],
        filenames: &["terminfo", "terminfo.src"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("names", &[
            rule(r"\n").token(T::Text).pop(1),
            rule(r"(,)([ \t]*)").groups(&[T::Punctuation, T::Text]).push(&["defs"]),
            rule(r"\|").token(T::Punctuation),
            rule(r"[^,|]+").token(T::NameAttribute),
        ]),
        ("defs", &[
            rule(r"\n[ \t]+").token(T::Text),
            rule(r"\n").token(T::Text).pop(2),
            rule(r"(#)([0-9]+)").groups(&[T::Operator, T::LiteralNumber]),
            rule(r"=").token(T::Operator).push(&["data"]),
            rule(r"(,)([ \t]*)").groups(&[T::Punctuation, T::Text]),
            rule(r"[^\s,=#]+").token(T::NameClass),
        ]),
        ("data", &[
            rule(r"\\[,\\]").token(T::Literal),
            rule(r"(,)([ \t]*)").groups(&[T::Punctuation, T::Text]).pop(1),
            rule(r"[^\\,]+").token(T::Literal),
            rule(r".").token(T::Literal),
        ]),
        ("root", &[
            rule(r"^#.*$").token(T::Comment),
            rule(r"^[^\s#,|]+").token(T::NameTag).push(&["names"]),
        ]),
    ],
};
