//! Chroma's `odin.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "odin",
    config: ConfigDef {
        name: "Odin",
        aliases: &["odin"],
        filenames: &["*.odin"],
        mime_types: &["text/odin"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("NestedComment", &[
            rule(r"/[*]").token(T::CommentMultiline).push(&[]),
            rule(r"[*]/").token(T::CommentMultiline).pop(1),
            rule(r"[\s\S]").token(T::CommentMultiline),
        ]),
        ("root", &[
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/[*]").token(T::CommentMultiline).push(&["NestedComment"]),
            rule(r"(import|package)\b").token(T::KeywordNamespace),
            rule(r"(proc|struct|map|enum|union)\b").token(T::KeywordDeclaration),
            rule(r"(asm|auto_cast|bit_set|break|case|cast|context|continue|defer|distinct|do|dynamic|else|enum|fallthrough|for|foreign|if|import|in|map|not_in|or_else|or_return|package|proc|return|struct|switch|transmute|typeid|union|using|when|where|panic|real|imag|len|cap|append|copy|delete|new|make|clearpanic|real|imag|len|cap|append|copy|delete|new|make|clear)\b").token(T::Keyword),
            rule(r"(true|false|nil)\b").token(T::KeywordConstant),
            rule(r"(uint|u8|u16|u32|u64|int|i8|i16|i32|i64|i16le|i32le|i64le|i128le|u16le|u32le|u64le|u128le|i16be|i32be|i64be|i128be|u16be|u32be|u64be|u128be|f16|f32|f64|complex32|complex64|complex128|quaternion64|quaternion128|quaternion256|byte|rune|string|cstring|typeid|any|bool|b8|b16|b32|b64|uintptr|rawptr)\b").token(T::KeywordType),
            rule(r"\#[a-zA-Z_]+\b").token(T::NameDecorator),
            rule(r"^\#\+\w+\s*$").token(T::NameAttribute),
            rule(r"^(\#\+\w+)(\s+)(\!)?([A-Za-z0-9-_!]+)(?:(,)(\!)?([A-Za-z0-9-_!]+))*\s*$").groups(&[T::NameAttribute, T::TextWhitespace, T::Operator, T::Name, T::Punctuation, T::Operator, T::Name]),
            rule(r"\@(\([a-zA-Z_]+\b\s*.*\)|\(?[a-zA-Z_]+\)?)").token(T::NameAttribute),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"([a-zA-Z_]\w*)(\s*)(\()").token(T::NameFunction),
            rule(r"[^\W\d]\w*").token(T::NameOther),
            rule(r"\d+i").token(T::LiteralNumber),
            rule(r"\d+\.\d*([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\.\d+([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\d+[Ee][-+]\d+i").token(T::LiteralNumber),
            rule(r"\d+(\.\d+[eE][+\-]?\d+|\.\d*|[eE][+\-]?\d+)").token(T::LiteralNumberFloat),
            rule(r"\.\d+([eE][+\-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"0o[0-7]+").token(T::LiteralNumberOct),
            rule(r"0x[0-9a-fA-F_]+").token(T::LiteralNumberHex),
            rule(r"0b[01_]+").token(T::LiteralNumberBin),
            rule(r"(0|[1-9][0-9_]*)").token(T::LiteralNumberInteger),
            rule(r#"'(\\['"\\abfnrtv]|\\x[0-9a-fA-F]{2}|\\[0-7]{1,3}|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}|[^\\])'"#).token(T::LiteralStringChar),
            rule(r"(`)([^`]*)(`)").token(T::LiteralString),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"(<<=|>>=|<<|>>|<=|>=|&=|&|\+=|-=|\*=|/=|%=|\||\^|=|&&|\|\||--|->|=|==|!=|:=|:|::|\.\.\<|\.\.=|[<>+\-*/%&])").token(T::Operator),
            rule(r"[{}()\[\],.;]").token(T::Punctuation),
        ]),
    ],
};
