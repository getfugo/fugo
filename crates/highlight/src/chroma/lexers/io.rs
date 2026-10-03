//! Chroma's `io.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "io",
    config: ConfigDef {
        name: "Io",
        aliases: &["io"],
        filenames: &["*.io"],
        mime_types: &["text/x-iosrc"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"//(.*?)\n").token(T::CommentSingle),
            rule(r"#(.*?)\n").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"/\+").token(T::CommentMultiline).push(&["nestedcomment"]),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"::=|:=|=|\(|\)|;|,|\*|-|\+|>|<|@|!|/|\||\^|\.|%|&|\[|\]|\{|\}").token(T::Operator),
            rule(r"(clone|do|doFile|doString|method|for|if|else|elseif|then)\b").token(T::Keyword),
            rule(r"(nil|false|true)\b").token(T::NameConstant),
            rule(r"(Object|list|List|Map|args|Sequence|Coroutine|File)\b").token(T::NameBuiltin),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"(\d+\.?\d*|\d*\.\d+)([eE][+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"\d+").token(T::LiteralNumberInteger),
        ]),
        ("nestedcomment", &[
            rule(r"[^+/]+").token(T::CommentMultiline),
            rule(r"/\+").token(T::CommentMultiline).push(&[]),
            rule(r"\+/").token(T::CommentMultiline).pop(1),
            rule(r"[+/]").token(T::CommentMultiline),
        ]),
    ],
};
