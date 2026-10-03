//! Chroma's `toml.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "toml",
    config: ConfigDef {
        name: "TOML",
        aliases: &["toml"],
        filenames: &["*.toml", "Pipfile", "poetry.lock"],
        mime_types: &["text/x-toml"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"#.*").token(T::Comment),
            rule(r"(false|true)\b").token(T::KeywordConstant),
            rule(r"\d\d\d\d-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d\+)?(Z|[+-]\d{2}:\d{2})").token(T::LiteralDate),
            rule(r"[+-]?[0-9](_?\d)*\.\d+").token(T::LiteralNumberFloat),
            rule(r"[+-]?[0-9](_?\d)*").token(T::LiteralNumberInteger),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"[.,=\[\]{}]").token(T::Punctuation),
            rule(r"[A-Za-z0-9_-]+").token(T::NameOther),
        ]),
    ],
};
