//! Chroma's `protocol_buffer.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "protocol_buffer",
    config: ConfigDef {
        name: "Protocol Buffer",
        aliases: &["protobuf", "proto"],
        filenames: &["*.proto"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("package", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameNamespace).pop(1),
            rule("").pop(1),
        ]),
        ("message", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameClass).pop(1),
            rule("").pop(1),
        ]),
        ("type", &[
            rule(r"[a-zA-Z_]\w*").token(T::Name).pop(1),
            rule("").pop(1),
        ]),
        ("root", &[
            rule(r"[ \t]+").token(T::Text),
            rule(r"[,;{}\[\]()<>]").token(T::Punctuation),
            rule(r"/(\\\n)?/(\n|(.|\n)*?[^\\]\n)").token(T::CommentSingle),
            rule(r"/(\\\n)?\*(.|\n)*?\*(\\\n)?/").token(T::CommentMultiline),
            rule(r"\b(extensions|required|repeated|optional|returns|default|option|packed|import|ctype|oneof|max|rpc|to)\b").token(T::Keyword),
            rule(r"(sfixed32|sfixed64|fixed32|fixed64|sint32|sint64|double|string|uint32|uint64|int32|float|int64|bytes|bool)\b").token(T::KeywordType),
            rule(r"(true|false)\b").token(T::KeywordConstant),
            rule(r"(package)(\s+)").groups(&[T::KeywordNamespace, T::Text]).push(&["package"]),
            rule(r"(message|extend)(\s+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["message"]),
            rule(r"(enum|group|service)(\s+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["type"]),
            rule(r#"\".*?\""#).token(T::LiteralString),
            rule(r"\'.*?\'").token(T::LiteralString),
            rule(r"(\d+\.\d*|\.\d+|\d+)[eE][+-]?\d+[LlUu]*").token(T::LiteralNumberFloat),
            rule(r"(\d+\.\d*|\.\d+|\d+[fF])[fF]?").token(T::LiteralNumberFloat),
            rule(r"(\-?(inf|nan))\b").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+[LlUu]*").token(T::LiteralNumberHex),
            rule(r"0[0-7]+[LlUu]*").token(T::LiteralNumberOct),
            rule(r"\d+[LlUu]*").token(T::LiteralNumberInteger),
            rule(r"[+-=]").token(T::Operator),
            rule(r"([a-zA-Z_][\w.]*)([ \t]*)(=)").groups(&[T::Name, T::Text, T::Operator]),
            rule(r"[a-zA-Z_][\w.]*").token(T::Name),
        ]),
    ],
};
