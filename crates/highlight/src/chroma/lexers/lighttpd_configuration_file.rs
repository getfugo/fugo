//! Chroma's `lighttpd_configuration_file.xml` lexer, converted to Rust
//! (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "lighttpd_configuration_file",
    config: ConfigDef {
        name: "Lighttpd configuration file",
        aliases: &["lighty", "lighttpd"],
        mime_types: &["text/x-lighttpd-conf"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"#.*\n").token(T::CommentSingle),
            rule(r"/\S*").token(T::Name),
            rule(r"[a-zA-Z._-]+").token(T::Keyword),
            rule(r"\d+\.\d+\.\d+\.\d+(?:/\d+)?").token(T::LiteralNumber),
            rule(r"[0-9]+").token(T::LiteralNumber),
            rule(r"=>|=~|\+=|==|=|\+").token(T::Operator),
            rule(r"\$[A-Z]+").token(T::NameBuiltin),
            rule(r"[(){}\[\],]").token(T::Punctuation),
            rule(r#""([^"\\]*(?:\\.[^"\\]*)*)""#).token(T::LiteralStringDouble),
            rule(r"\s+").token(T::Text),
        ]),
    ],
};
