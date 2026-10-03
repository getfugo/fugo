//! Chroma's `hexdump.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "hexdump",
    config: ConfigDef {
        name: "Hexdump",
        aliases: &["hexdump"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("offset", &[
            rule(r"^([0-9A-Ha-h]+)(:)").groups(&[T::NameLabel, T::Punctuation]).push(&["offset-mode"]),
            rule(r"^[0-9A-Ha-h]+").token(T::NameLabel),
        ]),
        ("offset-mode", &[
            rule(r"\s").token(T::Text).pop(1),
            rule(r"[0-9A-Ha-h]+").token(T::NameLabel),
            rule(r":").token(T::Punctuation),
        ]),
        ("piped-strings", &[
            rule(r"\n").token(T::Text),
            include("offset"),
            rule(r"[0-9A-Ha-h]{2}").token(T::LiteralNumberHex),
            rule(r"(\s{2,3})(\|)(.{1,16})(\|)$").groups(&[T::Text, T::Punctuation, T::LiteralString, T::Punctuation]),
            rule(r"\s").token(T::Text),
            rule(r"^\*").token(T::Punctuation),
        ]),
        ("bracket-strings", &[
            rule(r"\n").token(T::Text),
            include("offset"),
            rule(r"[0-9A-Ha-h]{2}").token(T::LiteralNumberHex),
            rule(r"(\s{2,3})(\>)(.{1,16})(\<)$").groups(&[T::Text, T::Punctuation, T::LiteralString, T::Punctuation]),
            rule(r"\s").token(T::Text),
            rule(r"^\*").token(T::Punctuation),
        ]),
        ("nonpiped-strings", &[
            rule(r"\n").token(T::Text),
            include("offset"),
            rule(r"([0-9A-Ha-h]{2})(\-)([0-9A-Ha-h]{2})").groups(&[T::LiteralNumberHex, T::Punctuation, T::LiteralNumberHex]),
            rule(r"[0-9A-Ha-h]{2}").token(T::LiteralNumberHex),
            rule(r"(\s{19,})(.{1,20}?)$").groups(&[T::Text, T::LiteralString]),
            rule(r"(\s{2,3})(.{1,20})$").groups(&[T::Text, T::LiteralString]),
            rule(r"\s").token(T::Text),
            rule(r"^\*").token(T::Punctuation),
        ]),
        ("root", &[
            rule(r"\n").token(T::Text),
            include("offset"),
            rule(r"([0-9A-Ha-h]{2})(\-)([0-9A-Ha-h]{2})").groups(&[T::LiteralNumberHex, T::Punctuation, T::LiteralNumberHex]),
            rule(r"[0-9A-Ha-h]{2}").token(T::LiteralNumberHex),
            rule(r"(\s{2,3})(\>)(.{16})(\<)$").groups(&[T::Text, T::Punctuation, T::LiteralString, T::Punctuation]).push(&["bracket-strings"]),
            rule(r"(\s{2,3})(\|)(.{16})(\|)$").groups(&[T::Text, T::Punctuation, T::LiteralString, T::Punctuation]).push(&["piped-strings"]),
            rule(r"(\s{2,3})(\>)(.{1,15})(\<)$").groups(&[T::Text, T::Punctuation, T::LiteralString, T::Punctuation]),
            rule(r"(\s{2,3})(\|)(.{1,15})(\|)$").groups(&[T::Text, T::Punctuation, T::LiteralString, T::Punctuation]),
            rule(r"(\s{2,3})(.{1,15})$").groups(&[T::Text, T::LiteralString]),
            rule(r"(\s{2,3})(.{16}|.{20})$").groups(&[T::Text, T::LiteralString]).push(&["nonpiped-strings"]),
            rule(r"\s").token(T::Text),
            rule(r"^\*").token(T::Punctuation),
        ]),
    ],
};
