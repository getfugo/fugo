//! Chroma's `lua.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "lua",
    config: ConfigDef {
        name: "Lua",
        aliases: &["lua", "luau"],
        filenames: &["*.lua", "*.wlua", "*.luau"],
        mime_types: &["text/x-lua", "application/x-lua"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("funcname", &[
            include("ws"),
            rule(r"[.:]").token(T::Punctuation),
            rule(r"(?:[^\W\d]\w*)(?=(?:(?:--\[(=*)\[[\w\W]*?\](\2)\])|(?:--.*$)|(?:\s+))*[.:])").token(T::NameClass),
            rule(r"(?:[^\W\d]\w*)").token(T::NameFunction).pop(1),
            rule(r"\(").token(T::Punctuation).pop(1),
        ]),
        ("label", &[
            include("ws"),
            rule(r"::").token(T::Punctuation).pop(1),
            rule(r"(?:[^\W\d]\w*)").token(T::NameLabel),
        ]),
        ("dqs", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"[^\\"]+"#).token(T::LiteralStringDouble),
        ]),
        ("root", &[
            rule(r"#!.*").token(T::CommentPreproc),
            rule("").push(&["base"]),
        ]),
        ("ws", &[
            rule(r"(?:--\[(=*)\[[\w\W]*?\](\1)\])").token(T::CommentMultiline),
            rule(r"(?:--.*$)").token(T::CommentSingle),
            rule(r"(?:\s+)").token(T::Text),
        ]),
        ("goto", &[
            include("ws"),
            rule(r"(?:[^\W\d]\w*)").token(T::NameLabel).pop(1),
        ]),
        ("sqs", &[
            rule(r"'").token(T::LiteralStringSingle).pop(1),
            rule(r"[^\\']+").token(T::LiteralStringSingle),
        ]),
        ("base", &[
            include("ws"),
            rule(r"(?i)0x[\da-f]*(\.[\da-f]*)?(p[+-]?\d+)?").token(T::LiteralNumberHex),
            rule(r"(?i)(\d*\.\d+|\d+\.\d*)(e[+-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"(?i)\d+e[+-]?\d+").token(T::LiteralNumberFloat),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"(?s)\[(=*)\[.*?\]\1\]").token(T::LiteralString),
            rule(r"::").token(T::Punctuation).push(&["label"]),
            rule(r"\.{3}").token(T::Punctuation),
            rule(r"[=<>|~&+\-*/%#^]+|\.\.").token(T::Operator),
            rule(r"[\[\]{}().,:;]").token(T::Punctuation),
            rule(r"(and|or|not)\b").token(T::OperatorWord),
            rule(r"(break|do|else|elseif|end|for|if|in|repeat|return|then|until|while)\b").token(T::KeywordReserved),
            rule(r"goto\b").token(T::KeywordReserved).push(&["goto"]),
            rule(r"(local)\b").token(T::KeywordDeclaration),
            rule(r"(true|false|nil)\b").token(T::KeywordConstant),
            rule(r"(function)\b").token(T::KeywordReserved).push(&["funcname"]),
            rule(r"[A-Za-z_]\w*(\.[A-Za-z_]\w*)?").token(T::Name),
            rule(r"'").token(T::LiteralStringSingle).combined(&["stringescape", "sqs"]),
            rule(r#"""#).token(T::LiteralStringDouble).combined(&["stringescape", "dqs"]),
        ]),
        ("stringescape", &[
            rule(r#"\\([abfnrtv\\"\']|[\r\n]{1,2}|z\s*|x[0-9a-fA-F]{2}|\d{1,3}|u\{[0-9a-fA-F]+\})"#).token(T::LiteralStringEscape),
        ]),
    ],
};
