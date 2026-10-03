//! Chroma's `tablegen.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "tablegen",
    config: ConfigDef {
        name: "TableGen",
        aliases: &["tablegen"],
        filenames: &["*.td"],
        mime_types: &["text/x-tablegen"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("whitespace", &[
            rule(r"(\n|\s)+").token(T::Text),
            rule(r"//.*?\n").token(T::Comment),
        ]),
        ("keyword", &[
            rule(r"(multiclass|foreach|string|class|field|defm|bits|code|list|def|int|let|dag|bit|in)\b").token(T::Keyword),
        ]),
        ("root", &[
            include("macro"),
            include("whitespace"),
            rule(r#"c?"[^"]*?""#).token(T::LiteralString),
            include("keyword"),
            rule(r"\$[_a-zA-Z][_\w]*").token(T::NameVariable),
            rule(r"\d*[_a-zA-Z][_\w]*").token(T::NameVariable),
            rule(r"\[\{[\w\W]*?\}\]").token(T::LiteralString),
            rule(r"[+-]?\d+|0x[\da-fA-F]+|0b[01]+").token(T::LiteralNumber),
            rule(r"[=<>{}\[\]()*.,!:;]").token(T::Punctuation),
        ]),
        ("macro", &[
            rule(r#"(#include\s+)("[^"]*")"#).groups(&[T::CommentPreproc, T::LiteralString]),
            rule(r"^\s*#(ifdef|ifndef)\s+[_\w][_\w\d]*").token(T::CommentPreproc),
            rule(r"^\s*#define\s+[_\w][_\w\d]*").token(T::CommentPreproc),
            rule(r"^\s*#endif").token(T::CommentPreproc),
        ]),
    ],
};
