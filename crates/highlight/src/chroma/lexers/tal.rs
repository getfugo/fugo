//! Chroma's `tal.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "tal",
    config: ConfigDef {
        name: "Tal",
        aliases: &["tal", "uxntal"],
        filenames: &["*.tal"],
        mime_types: &["text/x-uxntal"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("comment", &[
            rule(r"(?<!\S)\((?!\S)").token(T::CommentMultiline).push(&[]),
            rule(r"(?<!\S)\)(?!\S)").token(T::CommentMultiline).pop(1),
            rule(r"[^()]+").token(T::CommentMultiline),
            rule(r"[()]+").token(T::CommentMultiline),
        ]),
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"(?<!\S)\((?!\S)").token(T::CommentMultiline).push(&["comment"]),
            rule(r"(?<!\S)(BRK|LIT|INC|POP|DUP|NIP|SWP|OVR|ROT|EQU|NEQ|GTH|LTH|JMP|JCN|JSR|STH|LDZ|STZ|LDR|STR|LDA|STA|DEI|DEO|ADD|SUB|MUL|DIV|AND|ORA|EOR|SFT)2?k?r?(?!\S)").token(T::KeywordReserved),
            rule(r"[][{}](?!\S)").token(T::Punctuation),
            rule(r"#([0-9a-f]{2}){1,2}(?!\S)").token(T::LiteralNumberHex),
            rule(r#""\S+"#).token(T::LiteralString),
            rule(r"([0-9a-f]{2}){1,2}(?!\S)").token(T::Literal),
            rule(r"[|$][0-9a-f]{1,4}(?!\S)").token(T::KeywordDeclaration),
            rule(r"%\S+").token(T::NameDecorator),
            rule(r"@\S+").token(T::NameFunction),
            rule(r"&\S+").token(T::NameLabel),
            rule(r"/\S+").token(T::NameTag),
            rule(r"\.\S+").token(T::NameVariableMagic),
            rule(r",\S+").token(T::NameVariableInstance),
            rule(r";\S+").token(T::NameVariableGlobal),
            rule(r"-\S+").token(T::Literal),
            rule(r"_\S+").token(T::Literal),
            rule(r"=\S+").token(T::Literal),
            rule(r"!\S+").token(T::NameFunction),
            rule(r"\?\S+").token(T::NameFunction),
            rule(r"~\S+").token(T::KeywordNamespace),
            rule(r"\S+").token(T::NameFunction),
        ]),
    ],
};
