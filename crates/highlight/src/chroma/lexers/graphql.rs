//! Chroma's `graphql.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "graphql",
    config: ConfigDef {
        name: "GraphQL",
        aliases: &["graphql", "graphqls", "gql"],
        filenames: &["*.graphql", "*.graphqls"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(query|mutation|subscription|fragment|scalar|implements|interface|union|enum|input|type)").token(T::KeywordDeclaration).push(&["type"]),
            rule(r"(on|extend|schema|directive|\.\.\.)").token(T::KeywordDeclaration),
            rule(r"(QUERY|MUTATION|SUBSCRIPTION|FIELD|FRAGMENT_DEFINITION|FRAGMENT_SPREAD|INLINE_FRAGMENT|SCHEMA|SCALAR|OBJECT|FIELD_DEFINITION|ARGUMENT_DEFINITION|INTERFACE|UNION|ENUM|ENUM_VALUE|INPUT_OBJECT|INPUT_FIELD_DEFINITION)\b").token(T::KeywordConstant),
            rule(r"[^\W\d]\w*").token(T::NameProperty),
            rule(r"\@\w+").token(T::NameDecorator),
            rule(r":").token(T::Punctuation).push(&["type"]),
            rule(r"[\(\)\{\}\[\],!\|=]").token(T::Punctuation),
            rule(r"\$\w+").token(T::NameVariable),
            rule(r"\d+i").token(T::LiteralNumber),
            rule(r"\d+\.\d*([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\.\d+([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\d+[Ee][-+]\d+i").token(T::LiteralNumber),
            rule(r"\d+(\.\d+[eE][+\-]?\d+|\.\d*|[eE][+\-]?\d+)").token(T::LiteralNumberFloat),
            rule(r"\.\d+([eE][+\-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"(0|[1-9][0-9]*)").token(T::LiteralNumberInteger),
            rule(r#""""[\x00-\x7F]*?""""#).token(T::LiteralString),
            rule(r#""(\\["\\abfnrtv]|\\x[0-9a-fA-F]{2}|\\[0-7]{1,3}|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}|[^\\])""#).token(T::LiteralStringChar),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r#""(true|false|null)*""#).token(T::Literal),
            rule(r"[\r\n\s]+").token(T::TextWhitespace),
            rule(r"#[^\r\n]*").token(T::Comment),
        ]),
        ("type", &[
            rule(r"[^\W\d]\w*").token(T::NameClass).pop(1),
            include("root"),
        ]),
    ],
};
