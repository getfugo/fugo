//! Chroma's `whiley.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "whiley",
    config: ConfigDef {
        name: "Whiley",
        aliases: &["whiley"],
        filenames: &["*.whiley"],
        mime_types: &["text/x-whiley"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"/[*](.|\n)*?[*]/").token(T::CommentMultiline),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"(function|import|from|method|property|type|with|variant)\b").token(T::KeywordDeclaration),
            rule(r"(assert|assume|all|break|case|continue|debug|default|do|else|ensures|export|fail|final|for|if|in|is|native|no|new|private|protected|public|return|requires|skip|some|switch|unsafe|where|while)\b").token(T::Keyword),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(bool|byte|int|void)\b").token(T::KeywordType),
            rule(r"0b(?:_?[01])+").token(T::LiteralNumberBin),
            rule(r"0[xX][0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"(0|[1-9][0-9]*)").token(T::LiteralNumberInteger),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"[+%=><|^!?/\-*&~:]").token(T::Operator),
            rule(r"[{}()\[\],.;\|]").token(T::Punctuation),
        ]),
    ],
};
