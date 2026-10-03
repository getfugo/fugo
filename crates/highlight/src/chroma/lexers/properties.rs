//! Chroma's `properties.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "properties",
    config: ConfigDef {
        name: "properties",
        aliases: &["java-properties"],
        filenames: &["*.properties"],
        mime_types: &["text/x-java-properties"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^([ \t\f]*)([#!].*)").groups(&[T::Text, T::CommentSingle]),
            rule(r"^([ \t\f]*)(\S+?)([ \t\f]*)([=:])([ \t\f]*)(.*(?:(?<=\\)\n.*)*)").groups(&[T::Text, T::NameAttribute, T::Text, T::Operator, T::Text, T::LiteralString]),
            rule(r"^([ \t\f]*)(\S+)([ \t\f]+)(.*(?:(?<=\\)\n.*)*)").groups(&[T::Text, T::NameAttribute, T::Text, T::LiteralString]),
            rule(r"^([ \t\f]*)(\w+)$").groups(&[T::Text, T::NameAttribute]),
            rule(r"\n").token(T::Text),
        ]),
    ],
};
