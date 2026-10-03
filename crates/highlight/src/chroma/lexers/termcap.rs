//! Chroma's `termcap.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "termcap",
    config: ConfigDef {
        name: "Termcap",
        aliases: &["termcap"],
        filenames: &["termcap", "termcap.src"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("defs", &[
            rule(r"\\\n[ \t]*").token(T::Text),
            rule(r"\n[ \t]*").token(T::Text).pop(2),
            rule(r"(#)([0-9]+)").groups(&[T::Operator, T::LiteralNumber]),
            rule(r"=").token(T::Operator).push(&["data"]),
            rule(r":").token(T::Punctuation),
            rule(r"[^\s:=#]+").token(T::NameClass),
        ]),
        ("data", &[
            rule(r"\\072").token(T::Literal),
            rule(r":").token(T::Punctuation).pop(1),
            rule(r"[^:\\]+").token(T::Literal),
            rule(r".").token(T::Literal),
        ]),
        ("root", &[
            rule(r"^#.*$").token(T::Comment),
            rule(r"^[^\s#:|]+").token(T::NameTag).push(&["names"]),
        ]),
        ("names", &[
            rule(r"\n").token(T::Text).pop(1),
            rule(r":").token(T::Punctuation).push(&["defs"]),
            rule(r"\|").token(T::Punctuation),
            rule(r"[^:|]+").token(T::NameAttribute),
        ]),
    ],
};
