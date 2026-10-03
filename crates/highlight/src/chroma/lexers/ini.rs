//! Chroma's `ini.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "ini",
    config: ConfigDef {
        name: "INI",
        aliases: &["ini", "cfg", "dosini"],
        filenames: &[
            "*.ini",
            "*.cfg",
            "*.inf",
            "*.service",
            "*.socket",
            ".gitconfig",
            ".editorconfig",
            "pylintrc",
            ".pylintrc",
        ],
        mime_types: &["text/x-ini", "text/inf"],
        priority: 0.1, // higher priority than Inform 6
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"[;#].*").token(T::CommentSingle),
            rule(r"\[.*?\]$").token(T::Keyword),
            rule(r"(.*?)([ \t]*)(=)([ \t]*)(.*(?:\n[ \t].+)*)").groups(&[T::NameAttribute, T::Text, T::Operator, T::Text, T::LiteralString]),
            rule(r"(.+?)$").token(T::NameAttribute),
        ]),
    ],
};
