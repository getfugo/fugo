//! Chroma's `zed.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "zed",
    config: ConfigDef {
        name: "Zed",
        aliases: &["zed"],
        filenames: &["*.zed"],
        mime_types: &["text/zed"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/(\\\n)?[*][\w\W]*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"/(\\\n)?[*][\w\W]*").token(T::CommentMultiline),
            rule(r"(definition)\b").token(T::KeywordType),
            rule(r"(relation)\b").token(T::KeywordNamespace),
            rule(r"(permission)\b").token(T::KeywordDeclaration),
            rule(r"[a-zA-Z_]\w*/").token(T::NameNamespace),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"#[a-zA-Z_]\w*").token(T::NameVariable),
            rule(r"[+%=><|^!?/\-*&~:]").token(T::Operator),
            rule(r"[{}()\[\],.;]").token(T::Punctuation),
        ]),
    ],
};
