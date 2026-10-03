//! Chroma's `typoscripthtmldata.xml` lexer, converted to Rust
//! (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "typoscripthtmldata",
    config: ConfigDef {
        name: "TypoScriptHtmlData",
        aliases: &["typoscripthtmldata"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(INCLUDE_TYPOSCRIPT)").token(T::NameClass),
            rule(r#"(EXT|FILE|LLL):[^}\n"]*"#).token(T::LiteralString),
            rule(r"(.*)(###\w+###)(.*)").groups(&[T::LiteralString, T::NameConstant, T::LiteralString]),
            rule(r"(\{)(\$)((?:[\w\-]+\.)*)([\w\-]+)(\})").groups(&[T::LiteralStringSymbol, T::Operator, T::NameConstant, T::NameConstant, T::LiteralStringSymbol]),
            rule(r"(.*)(\{)([\w\-]+)(\s*:\s*)([\w\-]+)(\})(.*)").groups(&[T::LiteralString, T::LiteralStringSymbol, T::NameConstant, T::Operator, T::NameConstant, T::LiteralStringSymbol, T::LiteralString]),
            rule(r"\s+").token(T::Text),
            rule(r"[<>,:=.*%+|]").token(T::LiteralString),
            rule(r#"[\w"\-!/&;(){}#]+"#).token(T::LiteralString),
        ]),
    ],
};
