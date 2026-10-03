//! Chroma's `systemd.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "systemd",
    config: ConfigDef {
        name: "SYSTEMD",
        aliases: &["systemd"],
        filenames: &[
            "*.automount",
            "*.device",
            "*.dnssd",
            "*.link",
            "*.mount",
            "*.netdev",
            "*.network",
            "*.path",
            "*.scope",
            "*.service",
            "*.slice",
            "*.socket",
            "*.swap",
            "*.target",
            "*.timer",
        ],
        mime_types: &["text/plain"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"[;#].*").token(T::Comment),
            rule(r"\[.*?\]$").token(T::Keyword),
            rule(r"(.*?)(=)(.*)(\\\n)").groups(&[T::NameAttribute, T::Operator, T::LiteralString, T::Text]).push(&["continuation"]),
            rule(r"(.*?)(=)(.*)").groups(&[T::NameAttribute, T::Operator, T::LiteralString]),
        ]),
        ("continuation", &[
            rule(r"(.*?)(\\\n)").groups(&[T::LiteralString, T::Text]),
            rule(r"(.*)").token(T::LiteralString).pop(1),
        ]),
    ],
};
