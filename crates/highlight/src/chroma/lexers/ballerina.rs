//! Chroma's `ballerina.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "ballerina",
    config: ConfigDef {
        name: "Ballerina",
        aliases: &["ballerina"],
        filenames: &["*.bal"],
        mime_types: &["text/x-ballerina"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"(break|catch|continue|done|else|finally|foreach|forever|fork|if|lock|match|return|throw|transaction|try|while)\b").token(T::Keyword),
            rule(r"((?:(?:[^\W\d]|\$)[\w.\[\]$<>]*\s+)+?)((?:[^\W\d]|\$)[\w$]*)(\s*)(\()").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::Token(T::Text), E::Token(T::Operator)]),
            rule(r"@[^\W\d][\w.]*").token(T::NameDecorator),
            rule(r"(annotation|bind|but|endpoint|error|function|object|private|public|returns|service|type|var|with|worker)\b").token(T::KeywordDeclaration),
            rule(r"(boolean|byte|decimal|float|int|json|map|nil|record|string|table|xml)\b").token(T::KeywordType),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(import)(\s+)").groups(&[T::KeywordNamespace, T::Text]).push(&["import"]),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"'\\.'|'[^\\]'|'\\u[0-9a-fA-F]{4}'").token(T::LiteralStringChar),
            rule(r"(\.)((?:[^\W\d]|\$)[\w$]*)").groups(&[T::Operator, T::NameAttribute]),
            rule(r"^\s*([^\W\d]|\$)[\w$]*:").token(T::NameLabel),
            rule(r"([^\W\d]|\$)[\w$]*").token(T::Name),
            rule(r"([0-9][0-9_]*\.([0-9][0-9_]*)?|\.[0-9][0-9_]*)([eE][+\-]?[0-9][0-9_]*)?[fFdD]?|[0-9][eE][+\-]?[0-9][0-9_]*[fFdD]?|[0-9]([eE][+\-]?[0-9][0-9_]*)?[fFdD]|0[xX]([0-9a-fA-F][0-9a-fA-F_]*\.?|([0-9a-fA-F][0-9a-fA-F_]*)?\.[0-9a-fA-F][0-9a-fA-F_]*)[pP][+\-]?[0-9][0-9_]*[fFdD]?").token(T::LiteralNumberFloat),
            rule(r"0[xX][0-9a-fA-F][0-9a-fA-F_]*[lL]?").token(T::LiteralNumberHex),
            rule(r"0[bB][01][01_]*[lL]?").token(T::LiteralNumberBin),
            rule(r"0[0-7_]+[lL]?").token(T::LiteralNumberOct),
            rule(r"0|[1-9][0-9_]*[lL]?").token(T::LiteralNumberInteger),
            rule(r"[~^*!%&\[\](){}<>|+=:;,./?-]").token(T::Operator),
            rule(r"\n").token(T::Text),
        ]),
        ("import", &[
            rule(r"[\w.]+").token(T::NameNamespace).pop(1),
        ]),
    ],
};
