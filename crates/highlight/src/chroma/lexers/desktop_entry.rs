//! Chroma's `desktop_entry.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "desktop_entry",
    config: ConfigDef {
        name: "Desktop file",
        aliases: &["desktop", "desktop_entry"],
        filenames: &["*.desktop"],
        mime_types: &["application/x-desktop"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^[ \t]*\n").token(T::TextWhitespace),
            rule(r"^(#.*)(\n)").groups(&[T::CommentSingle, T::TextWhitespace]),
            rule(r"(\[[^\]\n]+\])(\n)").groups(&[T::Keyword, T::TextWhitespace]),
            rule(r"([-A-Za-z0-9]+)(\[[^\] \t=]+\])?([ \t]*)(=)([ \t]*)([^\n]*)([ \t\n]*\n)").groups(&[T::NameAttribute, T::NameNamespace, T::TextWhitespace, T::Operator, T::TextWhitespace, T::LiteralString, T::TextWhitespace]),
        ]),
    ],
};
