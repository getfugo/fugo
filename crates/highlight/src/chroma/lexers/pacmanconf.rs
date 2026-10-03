//! Chroma's `pacmanconf.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "pacmanconf",
    config: ConfigDef {
        name: "PacmanConf",
        aliases: &["pacmanconf"],
        filenames: &["pacman.conf"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"#.*$").token(T::CommentSingle),
            rule(r"^\s*\[.*?\]\s*$").token(T::Keyword),
            rule(r"(\w+)(\s*)(=)").groups(&[T::NameAttribute, T::Text, T::Operator]),
            rule(r"^(\s*)(\w+)(\s*)$").groups(&[T::Text, T::NameAttribute, T::Text]),
            rule(r"(\$repo|\$arch|%o|%u)\b").token(T::NameVariable),
            rule(r".").token(T::Text),
        ]),
    ],
};
