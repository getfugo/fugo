//! Chroma's `ebnf.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "ebnf",
    config: ConfigDef {
        name: "EBNF",
        aliases: &["ebnf"],
        filenames: &["*.ebnf"],
        mime_types: &["text/x-ebnf"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("comment", &[
            rule(r"[^*)]").token(T::CommentMultiline),
            include("comment_start"),
            rule(r"\*\)").token(T::CommentMultiline).pop(1),
            rule(r"[*)]").token(T::CommentMultiline),
        ]),
        ("identifier", &[
            rule(r"([a-zA-Z][\w \-]*)").token(T::Keyword),
        ]),
        ("root", &[
            include("whitespace"),
            include("comment_start"),
            include("identifier"),
            rule(r"=").token(T::Operator).push(&["production"]),
        ]),
        ("production", &[
            include("whitespace"),
            include("comment_start"),
            include("identifier"),
            rule(r#""[^"]*""#).token(T::LiteralStringDouble),
            rule(r"'[^']*'").token(T::LiteralStringSingle),
            rule(r"(\?[^?]*\?)").token(T::NameEntity),
            rule(r"[\[\]{}(),|]").token(T::Punctuation),
            rule(r"-").token(T::Operator),
            rule(r";").token(T::Punctuation).pop(1),
            rule(r"\.").token(T::Punctuation).pop(1),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::Text),
        ]),
        ("comment_start", &[
            rule(r"\(\*").token(T::CommentMultiline).push(&["comment"]),
        ]),
    ],
};
