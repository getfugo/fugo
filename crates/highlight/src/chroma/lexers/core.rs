//! Chroma's `core.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "core",
    config: ConfigDef {
        name: "Core",
        aliases: &["core"],
        filenames: &["*.core"],
        mime_types: &["text/x-core"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"//(.*?)\n").token(T::CommentSingle),
            rule(r"(const|class|value|union|enum|trait|impl|annotation)\b").token(T::KeywordDeclaration),
            rule(r"(fun|let|var)\b").token(T::KeywordDeclaration),
            rule(r"(mod|use)\b").token(T::KeywordNamespace),
            rule(r"(if|else|is|for|in|while|return)\b").token(T::Keyword),
            rule(r"(true|false|self)\b").token(T::KeywordConstant),
            rule(r"0[b][01](_?[01])*(i32|i64|u8|f32|f64)?").token(T::LiteralNumberBin),
            rule(r"0[x][\da-fA-F](_?[\dA-Fa-f])*(i32|i64|u8|f32|f64)?").token(T::LiteralNumberHex),
            rule(r"\d(_?\d)*\.\d(_?\d)*([eE][-+]?\d(_?\d)*)?(f32|f64)?").token(T::LiteralNumberFloat),
            rule(r"\d(_?\d)*(i32|i64|u8|f32|f64)?").token(T::LiteralNumberInteger),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"@([a-z_]\w*[!?]?)").token(T::NameAttribute),
            rule(r"===|!==|==|!=|>=|<=|[><*/+-=&|^]").token(T::Operator),
            rule(r"[A-Z][A-Za-z0-9_]*").token(T::NameClass),
            rule(r"([a-z_]\w*[!?]?)").token(T::Name),
            rule(r"[(){}\[\],.;]").token(T::Punctuation),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\["\\fnrt]|\\u\{[\da-fA-F]{1,6}\}"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"]+"#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
    ],
};
