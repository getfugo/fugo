//! Chroma's `bnf.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "bnf",
    config: ConfigDef {
        name: "BNF",
        aliases: &["bnf"],
        filenames: &["*.bnf"],
        mime_types: &["text/x-bnf"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(<)([ -;=?-~]+)(>)").groups(&[T::Punctuation, T::NameClass, T::Punctuation]),
            rule(r"::=").token(T::Operator),
            rule(r"[^<>:]+").token(T::Text),
            rule(r".").token(T::Text),
        ]),
    ],
};
