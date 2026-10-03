//! Chroma's `xorg.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "xorg",
    config: ConfigDef {
        name: "Xorg",
        aliases: &["xorg.conf"],
        filenames: &["xorg.conf"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"#.*$").token(T::Comment),
            rule(r#"((|Sub)Section)(\s+)("\w+")"#).groups(&[T::KeywordNamespace, T::LiteralStringEscape, T::TextWhitespace, T::LiteralStringEscape]),
            rule(r"(End(|Sub)Section)").token(T::KeywordNamespace),
            rule(r"(\w+)(\s+)([^\n#]+)").groups(&[T::NameKeyword, T::TextWhitespace, T::LiteralString]),
        ]),
    ],
};
