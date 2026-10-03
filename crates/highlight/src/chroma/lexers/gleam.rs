//! Chroma's `gleam.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "gleam",
    config: ConfigDef {
        name: "Gleam",
        aliases: &["gleam"],
        filenames: &["*.gleam"],
        mime_types: &["text/x-gleam"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"///(.*?)\n").token(T::LiteralStringDoc),
            rule(r"//(.*?)\n").token(T::CommentSingle),
            rule(r"(as|assert|case|opaque|panic|pub|todo)\b").token(T::Keyword),
            rule(r"(import|use)\b").token(T::KeywordNamespace),
            rule(r"(auto|const|delegate|derive|echo|else|if|implement|macro|test)\b").token(T::KeywordReserved),
            rule(r"(let)\b").token(T::KeywordDeclaration),
            rule(r"(fn)\b").token(T::Keyword),
            rule(r"(type)\b").token(T::Keyword),
            rule(r"(True|False)\b").token(T::KeywordConstant),
            rule(r"0[bB][01](_?[01])*").token(T::LiteralNumberBin),
            rule(r"0[oO][0-7](_?[0-7])*").token(T::LiteralNumberOct),
            rule(r"0[xX][\da-fA-F](_?[\dA-Fa-f])*").token(T::LiteralNumberHex),
            rule(r"\d(_?\d)*\.\d(_?\d)*([eE][-+]?\d(_?\d)*)?").token(T::LiteralNumberFloat),
            rule(r"\d(_?\d)*").token(T::LiteralNumberInteger),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"@([a-z_]\w*[!?]?)").token(T::NameAttribute),
            rule(r"[{}()\[\],]|[#(]|\.\.|<>|<<|>>").token(T::Punctuation),
            rule(r":|->").token(T::Operator),
            rule(r"[+\-*/%!=<>&|.]|<-").token(T::Operator),
            rule(r"([a-z_][A-Za-z0-9_]*)(\()").groups(&[T::NameFunction, T::Punctuation]),
            rule(r"[A-Z][A-Za-z0-9_]*").token(T::NameClass),
            rule(r"([a-z_]\w*[!?]?)").token(T::Name),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\["\\fnrt]|\\u\{[\da-fA-F]{1,6}\}"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"]+"#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
    ],
};
