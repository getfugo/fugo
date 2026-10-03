//! Chroma's `cue.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "cue",
    config: ConfigDef {
        name: "CUE",
        aliases: &["cue"],
        filenames: &["*.cue"],
        mime_types: &["text/x-cue"],
        dot_all: true,
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"//[^\n\r]+").token(T::CommentSingle),
            rule(r"\n").token(T::Text),
            rule(r"(\+|&&|==|<|=|-|\|\||!=|>|:|\*|&|=~|<=|\?|\[|\]|,|/|\||!~|>=|!|_\|_|\.\.\.)").token(T::Operator),
            rule(r#"#*"+"#).token(T::LiteralString).push(&["string"]),
            rule(r"'(\\\\|\\'|[^'\n])*['\n]").token(T::LiteralString),
            rule(r"0[boxX][0-9a-fA-F][_0-9a-fA-F]*|(\.\d+|\d[_\d]*(\.\d*)?)([eE][+-]?\d+)?[KMGTP]?i?").token(T::LiteralNumber),
            rule(r"[~!%^&*()+=|\[\]:;,.<>/?-]").token(T::Punctuation),
            rule(r"[{}]").token(T::Punctuation),
            rule(r"(import|for|if|in|let|package)\b").token(T::Keyword),
            rule(r"(bool|float|int|string|uint|ulong|ushort)\b\??").token(T::KeywordType),
            rule(r"(true|false|null|_)\b").token(T::KeywordConstant),
            rule(r"[@#]?[_a-zA-Z$]\w*").token(T::Name),
        ]),
        ("string", &[
            rule(r"\\#*\(").token(T::LiteralStringInterpol).push(&["string-intp"]),
            rule(r#""+#*"#).token(T::LiteralString).pop(1),
            rule(r#"\\['"\\nrt]|\\x[0-9a-fA-F]{2}|\\[0-7]{1,3}|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"]+"#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
        ("string-intp", &[
            rule(r"\)").token(T::LiteralStringInterpol).pop(1),
            include("root"),
        ]),
    ],
};
