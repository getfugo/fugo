//! Chroma's `http.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "http",
    config: ConfigDef {
        name: "HTTP",
        aliases: &["http"],
        dot_all: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("content", &[
            rule(r".+").emit_func("httpContentBlock"),
        ]),
        ("headers", &[
            rule(r"([^\s:]+)( *)(:)( *)([^\r\n]+)(\r?\n|\Z)").emit_func("httpHeaderBlock"),
            rule(r"([\t ]+)([^\r\n]+)(\r?\n|\Z)").emit_func("httpContinuousHeaderBlock"),
            rule(r"\r?\n").token(T::Text).push(&["content"]),
        ]),
        ("root", &[
            rule(r"(GET|POST|PUT|DELETE|HEAD|OPTIONS|TRACE|PATCH|CONNECT)( +)([^ ]+)( +)(HTTP)(/)([123](?:\.[01])?)(\r?\n|\Z)").groups(&[T::NameFunction, T::Text, T::NameNamespace, T::Text, T::KeywordReserved, T::Operator, T::LiteralNumber, T::Text]).push(&["headers"]),
            rule(r"(HTTP)(/)([123](?:\.[01])?)( +)(\d{3})( *)([^\r\n]*)(\r?\n|\Z)").groups(&[T::KeywordReserved, T::Operator, T::LiteralNumber, T::Text, T::LiteralNumber, T::Text, T::NameException, T::Text]).push(&["headers"]),
        ]),
    ],
};
