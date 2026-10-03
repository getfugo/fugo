//! Chroma's `handlebars.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "handlebars",
    config: ConfigDef {
        name: "Handlebars",
        aliases: &["handlebars", "hbs"],
        filenames: &["*.handlebars", "*.hbs"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"[^{]+").token(T::Other),
            rule(r"\{\{!.*\}\}").token(T::Comment),
            rule(r"(\{\{\{)(\s*)").groups(&[T::CommentSpecial, T::Text]).push(&["tag"]),
            rule(r"(\{\{)(\s*)").groups(&[T::CommentPreproc, T::Text]).push(&["tag"]),
        ]),
        ("tag", &[
            rule(r"\s+").token(T::Text),
            rule(r"\}\}\}").token(T::CommentSpecial).pop(1),
            rule(r"\}\}").token(T::CommentPreproc).pop(1),
            rule(r"([#/]*)(each|if|unless|else|with|log|in(?:line)?)").groups(&[T::Keyword, T::Keyword]),
            rule(r"#\*inline").token(T::Keyword),
            rule(r"([#/])([\w-]+)").groups(&[T::NameFunction, T::NameFunction]),
            rule(r"([\w-]+)(=)").groups(&[T::NameAttribute, T::Operator]),
            rule(r"(>)(\s*)(@partial-block)").groups(&[T::Keyword, T::Text, T::Keyword]),
            rule(r"(#?>)(\s*)([\w-]+)").groups(&[T::Keyword, T::Text, T::NameVariable]),
            rule(r"(>)(\s*)(\()").groups(&[T::Keyword, T::Text, T::Punctuation]).push(&["dynamic-partial"]),
            include("generic"),
        ]),
        ("dynamic-partial", &[
            rule(r"\s+").token(T::Text),
            rule(r"\)").token(T::Punctuation).pop(1),
            rule(r"(lookup)(\s+)(\.|this)(\s+)").groups(&[T::Keyword, T::Text, T::NameVariable, T::Text]),
            rule(r"(lookup)(\s+)(\S+)").bygroups(&[E::Token(T::Keyword), E::Token(T::Text), E::UsingSelf("variable")]),
            rule(r"[\w-]+").token(T::NameFunction),
            include("generic"),
        ]),
        ("variable", &[
            rule(r"[a-zA-Z][\w-]*").token(T::NameVariable),
            rule(r"\.[\w-]+").token(T::NameVariable),
            rule(r"(this\/|\.\/|(\.\.\/)+)[\w-]+").token(T::NameVariable),
        ]),
        ("generic", &[
            include("variable"),
            rule(r#":?"(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r":?'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"[0-9](\.[0-9]*)?(eE[+-][0-9])?[flFLdD]?|0[xX][0-9a-fA-F]+[Ll]?").token(T::LiteralNumber),
        ]),
    ],
};
