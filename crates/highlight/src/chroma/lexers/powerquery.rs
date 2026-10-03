//! Chroma's `powerquery.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "powerquery",
    config: ConfigDef {
        name: "PowerQuery",
        aliases: &["powerquery", "pq"],
        filenames: &["*.pq"],
        mime_types: &["text/x-powerquery"],
        case_insensitive: true,
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"(and|as|each|else|error|false|if|in|is|let|meta|not|null|or|otherwise|section|shared|then|true|try|type)\b").token(T::Keyword),
            rule(r"(#binary|#date|#datetime|#datetimezone|#duration|#infinity|#nan|#sections|#shared|#table|#time)\b").token(T::KeywordType),
            rule(r#"(([a-zA-Z]|_)[\w|._]*|#"[^"]+")"#).token(T::Name),
            rule(r"0[xX][0-9a-fA-F][0-9a-fA-F_]*[lL]?").token(T::LiteralNumberHex),
            rule(r"([0-9]+\.[0-9]+|\.[0-9]+)([eE][0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"[\(\)\[\]\{\}]").token(T::Punctuation),
            rule(r"\.\.|\.\.\.|=>|<=|>=|<>|[@!?,;=<>\+\-\*\/&]").token(T::Operator),
        ]),
    ],
};
