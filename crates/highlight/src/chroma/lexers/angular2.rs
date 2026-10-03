//! Chroma's `angular2.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "angular2",
    config: ConfigDef {
        name: "Angular2",
        aliases: &["ng2"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("attr", &[
            rule(r#"".*?""#).token(T::LiteralString).pop(1),
            rule(r"'.*?'").token(T::LiteralString).pop(1),
            rule(r"[^\s>]+").token(T::LiteralString).pop(1),
        ]),
        ("root", &[
            rule(r"[^{([*#]+").token(T::Other),
            rule(r"(\{\{)(\s*)").groups(&[T::CommentPreproc, T::Text]).push(&["ngExpression"]),
            rule(r"([([]+)([\w:.-]+)([\])]+)(\s*)(=)(\s*)").groups(&[T::Punctuation, T::NameAttribute, T::Punctuation, T::Text, T::Operator, T::Text]).push(&["attr"]),
            rule(r"([([]+)([\w:.-]+)([\])]+)(\s*)").groups(&[T::Punctuation, T::NameAttribute, T::Punctuation, T::TextWhitespace]),
            rule(r"([*#])([\w:.-]+)(\s*)(=)(\s*)").groups(&[T::Punctuation, T::NameAttribute, T::Punctuation, T::Operator, T::TextWhitespace]).push(&["attr"]),
            rule(r"([*#])([\w:.-]+)(\s*)").groups(&[T::Punctuation, T::NameAttribute, T::Punctuation]),
        ]),
        ("ngExpression", &[
            rule(r"\s+(\|\s+)?").token(T::Text),
            rule(r"\}\}").token(T::CommentPreproc).pop(1),
            rule(r":?(true|false)").token(T::LiteralStringBoolean),
            rule(r#":?"(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r":?'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"[0-9](\.[0-9]*)?(eE[+-][0-9])?[flFLdD]?|0[xX][0-9a-fA-F]+[Ll]?").token(T::LiteralNumber),
            rule(r"[a-zA-Z][\w-]*(\(.*\))?").token(T::NameVariable),
            rule(r"\.[\w-]+(\(.*\))?").token(T::NameVariable),
            rule(r"(\?)(\s*)([^}\s]+)(\s*)(:)(\s*)([^}\s]+)(\s*)").groups(&[T::Operator, T::Text, T::LiteralString, T::Text, T::Operator, T::Text, T::LiteralString, T::Text]),
        ]),
    ],
};
