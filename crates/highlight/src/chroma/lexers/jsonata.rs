//! Chroma's `jsonata.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "jsonata",
    config: ConfigDef {
        name: "JSONata",
        aliases: &["jsonata"],
        filenames: &["*.jsonata"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"[{}()\[\]:;,\.=]").token(T::Punctuation),
            // // Spread operator
            rule(r"\.\.").token(T::Operator),
            // // Sort operator
            rule(r"\^(?=\()").token(T::Operator),
            // // Descendant | Wildcard | Multiplication
            rule(r"\*\*|\*(?=\.)|\*").token(T::Operator),
            // // Division
            rule(r"\/(?!\*)").token(T::Operator),
            // // Comparison operators
            rule(r"[<>!]=?").token(T::Operator),
            rule(r"~>").token(T::Operator),
            rule(r"\b(and|or|in)\b").token(T::Operator),
            rule(r"[%@#&?]|\+(?!\d)|\-(?!\d)").token(T::Operator),
            rule(r"\$[a-zA-Z0-9_]*(?![\w\(])").token(T::NameVariable),
            rule(r"\$\w*(?=\()").token(T::NameFunction),
            rule(r"\s+").token(T::Text),
            rule(r"(true|false)\b").token(T::KeywordConstant),
            rule(r"\b(function)\b").token(T::Keyword),
            rule(r"(\+|-)?(0|[1-9]\d*)(\.\d+[eE](\+|-)?\d+|[eE](\+|-)?\d+|\.\d+)").token(T::LiteralNumberFloat),
            rule(r"(\+|-)?(0|[1-9]\d*)").token(T::LiteralNumberInteger),
            // NOTE: This expression matches all object keys (NameTags), which are essentially strings with double quotes
            // that should only be captured on the left side of a colon (:) within a JSON-like object.
            // Therefore, this expression must preceed the one for all LiteralStringDouble
            rule(r#""(\\.|[^\\"\r\n])*"(?=\s*:)"#).token(T::NameTag),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"`.*`").token(T::LiteralStringBacktick),
            // NOTE: This expression matches everything remaining, which should be only JSONata names.
            // Therefore, it has been left as last intentionally
            rule(r"[a-zA-Z0-9_]*").token(T::Name),
        ]),
    ],
};
