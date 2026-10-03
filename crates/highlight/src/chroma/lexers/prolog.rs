//! Chroma's `prolog.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "prolog",
    config: ConfigDef {
        name: "Prolog",
        aliases: &["prolog"],
        filenames: &["*.ecl", "*.prolog", "*.pro", "*.pl"],
        mime_types: &["text/x-prolog"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"/\*").token(T::CommentMultiline).push(&["nested-comment"]),
            rule(r"%.*").token(T::CommentSingle),
            rule(r"0\'.").token(T::LiteralStringChar),
            rule(r"0b[01]+").token(T::LiteralNumberBin),
            rule(r"0o[0-7]+").token(T::LiteralNumberOct),
            rule(r"0x[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"\d\d?\'[a-zA-Z0-9]+").token(T::LiteralNumberInteger),
            rule(r"(\d+\.\d*|\d*\.\d+)([eE][+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"[\[\](){}|.,;!]").token(T::Punctuation),
            rule(r":-|-->").token(T::Punctuation),
            rule(r#""(?:\\x[0-9a-fA-F]+\\|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}|\\[0-7]+\\|\\["\nabcefnrstv]|[^\\"])*""#).token(T::LiteralStringDouble),
            rule(r"'(?:''|[^'])*'").token(T::LiteralStringAtom),
            rule(r"is\b").token(T::Operator),
            rule(r"(<|>|=<|>=|==|=:=|=|/|//|\*|\+|-)(?=\s|[a-zA-Z0-9\[])").token(T::Operator),
            rule(r"(mod|div|not)\b").token(T::Operator),
            rule(r"_").token(T::Keyword),
            rule(r"([a-z]+)(:)").groups(&[T::NameNamespace, T::Punctuation]),
            rule(r"([a-zÀ-῿぀-퟿-￯][\w$À-῿぀-퟿-￯]*)(\s*)(:-|-->)").groups(&[T::NameFunction, T::Text, T::Operator]),
            rule(r"([a-zÀ-῿぀-퟿-￯][\w$À-῿぀-퟿-￯]*)(\s*)(\()").groups(&[T::NameFunction, T::Text, T::Punctuation]),
            rule(r"[a-zÀ-῿぀-퟿-￯][\w$À-῿぀-퟿-￯]*").token(T::LiteralStringAtom),
            rule(r"[#&*+\-./:<=>?@\\^~¡-¿‐-〿]+").token(T::LiteralStringAtom),
            rule(r"[A-Z_]\w*").token(T::NameVariable),
            rule(r"\s+|[ -‏￰-�￯]").token(T::Text),
        ]),
        ("nested-comment", &[
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"/\*").token(T::CommentMultiline).push(&[]),
            rule(r"[^*/]+").token(T::CommentMultiline),
            rule(r"[*/]").token(T::CommentMultiline),
        ]),
    ],
};
