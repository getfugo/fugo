//! Chroma's `genshi_text.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "genshi_text",
    config: ConfigDef {
        name: "Genshi Text",
        aliases: &["genshitext"],
        mime_types: &["application/x-genshi-text", "text/x-genshi"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("directive", &[
            rule(r"\n").token(T::Text).pop(1),
            rule(r"(?:def|for|if)\s+.*").using("Python").pop(1),
            rule(r"(choose|when|with)([^\S\n]+)(.*)").bygroups(&[E::Token(T::Keyword), E::Token(T::Text), E::Using("Python")]).pop(1),
            rule(r"(choose|otherwise)\b").token(T::Keyword).pop(1),
            rule(r"(end\w*)([^\S\n]*)(.*)").groups(&[T::Keyword, T::Text, T::Comment]).pop(1),
        ]),
        ("root", &[
            rule(r"[^#$\s]+").token(T::Other),
            rule(r"^(\s*)(##.*)$").groups(&[T::Text, T::Comment]),
            rule(r"^(\s*)(#)").groups(&[T::Text, T::CommentPreproc]).push(&["directive"]),
            include("variable"),
            rule(r"[#$\s]").token(T::Other),
        ]),
        ("variable", &[
            rule(r"(?<!\$)(\$\{)(.+?)(\})").bygroups(&[E::Token(T::CommentPreproc), E::Using("Python"), E::Token(T::CommentPreproc)]),
            rule(r"(?<!\$)(\$)([a-zA-Z_][\w.]*)").token(T::NameVariable),
        ]),
    ],
};
