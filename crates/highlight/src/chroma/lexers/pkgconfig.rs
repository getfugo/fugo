//! Chroma's `pkgconfig.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "pkgconfig",
    config: ConfigDef {
        name: "PkgConfig",
        aliases: &["pkgconfig"],
        filenames: &["*.pc"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("curly", &[
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
            rule(r"\w+").token(T::NameAttribute),
        ]),
        ("spvalue", &[
            include("interp"),
            rule(r"#.*$").token(T::CommentSingle).pop(1),
            rule(r"\n").token(T::Text).pop(1),
            rule(r"[^${}#\n]+").token(T::Text),
            rule(r".").token(T::Text),
        ]),
        ("root", &[
            rule(r"#.*$").token(T::CommentSingle),
            rule(r"^(\w+)(=)").groups(&[T::NameAttribute, T::Operator]),
            rule(r"^([\w.]+)(:)").groups(&[T::NameTag, T::Punctuation]).push(&["spvalue"]),
            include("interp"),
            rule(r"[^${}#=:\n.]+").token(T::Text),
            rule(r".").token(T::Text),
        ]),
        ("interp", &[
            rule(r"\$\$").token(T::Text),
            rule(r"\$\{").token(T::LiteralStringInterpol).push(&["curly"]),
        ]),
    ],
};
