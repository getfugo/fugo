//! Chroma's `hare.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "hare",
    config: ConfigDef {
        name: "Hare",
        aliases: &["hare"],
        filenames: &["*.ha"],
        mime_types: &["text/x-hare"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\0abfnrtv"']|x[a-fA-F0-9]{2}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{8})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
        ("root", &[
            rule(r"[\s\n]+").token(T::TextWhitespace),
            rule(r"@[a-z]+").token(T::NameDecorator),
            rule(r"//.*\n").token(T::CommentSingle),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"`[^`]*`").token(T::LiteralString),
            rule(r#"'(\\[\\0abfnrtv"']||\\(x[a-fA-F0-9]{2}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{8})|[^\\'])'"#).token(T::LiteralStringChar),
            rule(r"(0|[1-9]\d*)\.\d+([eE][+-]?\d+)?(f32|f64)?").token(T::LiteralNumberFloat),
            rule(r"(0|[1-9]\d*)([eE][+-]?\d+)?(f32|f64)").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+\.[0-9a-fA-F]+([pP][+-]?\d+(f32|f64)?)?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+[pP][+-]?\d+(f32|f64)").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+(z|[iu](8|16|32|64)?)?").token(T::LiteralNumberHex),
            rule(r"0o[0-7]+(z|[iu](8|16|32|64)?)?").token(T::LiteralNumberOct),
            rule(r"0b[01]+(z|[iu](8|16|32|64)?)?").token(T::LiteralNumberBin),
            rule(r"(0|[1-9]\d*)([eE][+-]?\d+)?(z|[iu](8|16|32|64)?)?").token(T::LiteralNumberInteger),
            rule(r"[~!%^&*+=|?:<>/-]|[ai]s\b|\.\.\.").token(T::Operator),
            rule(r"[()\[\],.{};]").token(T::Punctuation),
            rule(r"use\b").token(T::KeywordNamespace),
            rule(r"(_|align|break|const|continue|else|enum|export|for|if|return|static|struct|offset|union|fn|free|assert|abort|alloc|let|len|def|type|match|switch|case|append|delete|insert|defer|yield|vastart|vaarg|vaend)\b").token(T::Keyword),
            rule(r"(size)([\s\n]*)(\()").groups(&[T::Keyword, T::TextWhitespace, T::Punctuation]),
            rule(r"(str|size|rune|bool|int|uint|uintptr|u8|u16|u32|u64|i8|i16|i32|i64|f32|f64|null|void|done|nullable|valist|opaque|never)\b").token(T::KeywordType),
            rule(r"(true|false)\b").token(T::NameBuiltin),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
    ],
};
