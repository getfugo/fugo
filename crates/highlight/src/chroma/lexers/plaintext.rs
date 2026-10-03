//! Chroma's `plaintext.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "plaintext",
    config: ConfigDef {
        name: "plaintext",
        aliases: &["text", "plain", "no-highlight"],
        filenames: &["*.txt"],
        mime_types: &["text/plain"],
        priority: -1.0,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r".+").token(T::Text),
            rule(r"\n").token(T::Text),
        ]),
    ],
};
