//! Chroma's `typoscriptcssdata.xml` lexer, converted to Rust
//! (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "typoscriptcssdata",
    config: ConfigDef {
        name: "TypoScriptCssData",
        aliases: &["typoscriptcssdata"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(.*)(###\w+###)(.*)").groups(&[T::LiteralString, T::NameConstant, T::LiteralString]),
            rule(r"(\{)(\$)((?:[\w\-]+\.)*)([\w\-]+)(\})").groups(&[T::LiteralStringSymbol, T::Operator, T::NameConstant, T::NameConstant, T::LiteralStringSymbol]),
            rule(r"(.*)(\{)([\w\-]+)(\s*:\s*)([\w\-]+)(\})(.*)").groups(&[T::LiteralString, T::LiteralStringSymbol, T::NameConstant, T::Operator, T::NameConstant, T::LiteralStringSymbol, T::LiteralString]),
            rule(r"\s+").token(T::Text),
            rule(r"/\*(?:(?!\*/).)*\*/").token(T::Comment),
            rule(r#"(?<!(#|\'|"))(?:#(?!(?:[a-fA-F0-9]{6}|[a-fA-F0-9]{3}))[^\n#]+|//[^\n]*)"#).token(T::Comment),
            rule(r"[<>,:=.*%+|]").token(T::LiteralString),
            rule(r#"[\w"\-!/&;(){}]+"#).token(T::LiteralString),
        ]),
    ],
};
