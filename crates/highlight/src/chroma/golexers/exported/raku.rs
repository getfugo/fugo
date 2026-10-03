//! Chroma's `raku.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "raku",
    config: ConfigDef {
        name: "Raku",
        aliases: &["perl6", "pl6", "raku"],
        filenames: &[
            "*.pl",
            "*.pm",
            "*.nqp",
            "*.p6",
            "*.6pl",
            "*.p6l",
            "*.pl6",
            "*.6pm",
            "*.p6m",
            "*.pm6",
            "*.t",
            "*.raku",
            "*.rakumod",
            "*.rakutest",
            "*.rakudoc",
        ],
        mime_types: &["text/x-perl6", "application/x-perl6", "text/x-raku", "application/x-raku"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r".+").token(T::Text),
            rule(r"\n").token(T::Text),
        ]),
    ],
};
