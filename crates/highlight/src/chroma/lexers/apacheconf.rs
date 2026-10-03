//! Chroma's `apacheconf.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "apacheconf",
    config: ConfigDef {
        name: "ApacheConf",
        aliases: &["apacheconf", "aconf", "apache"],
        filenames: &[".htaccess", "apache.conf", "apache2.conf"],
        mime_types: &["text/x-apacheconf"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"(#.*?)$").token(T::Comment),
            rule(r"(<[^\s>]+)(?:(\s+)(.*?))?(>)").groups(&[T::NameTag, T::Text, T::LiteralString, T::NameTag]),
            rule(r"([a-z]\w*)(\s+)").groups(&[T::NameBuiltin, T::Text]).push(&["value"]),
            rule(r"\.+").token(T::Text),
        ]),
        ("value", &[
            rule(r"\\\n").token(T::Text),
            rule(r"$").token(T::Text).pop(1),
            rule(r"\\").token(T::Text),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\d+\.\d+\.\d+\.\d+(?:/\d+)?").token(T::LiteralNumber),
            rule(r"\d+").token(T::LiteralNumber),
            rule(r"/([a-z0-9][\w./-]+)").token(T::LiteralStringOther),
            rule(r"(on|off|none|any|all|double|email|dns|min|minimal|os|productonly|full|emerg|alert|crit|error|warn|notice|info|debug|registry|script|inetd|standalone|user|group)\b").token(T::Keyword),
            rule(r#""([^"\\]*(?:\\.[^"\\]*)*)""#).token(T::LiteralStringDouble),
            rule(r#"[^\s"\\]+"#).token(T::Text),
        ]),
    ],
};
