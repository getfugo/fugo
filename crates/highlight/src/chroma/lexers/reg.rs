//! Chroma's `reg.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "reg",
    config: ConfigDef {
        name: "reg",
        aliases: &["registry"],
        filenames: &["*.reg"],
        mime_types: &["text/x-windows-registry"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"Windows Registry Editor.*").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"[;#].*").token(T::CommentSingle),
            rule(r"(\[)(-?)(HKEY_[A-Z_]+)(.*?\])$").groups(&[T::Keyword, T::Operator, T::NameBuiltin, T::Keyword]),
            rule(r#"("(?:\\"|\\\\|[^"])+")([ \t]*)(=)([ \t]*)"#).groups(&[T::NameAttribute, T::Text, T::Operator, T::Text]).push(&["value"]),
            rule(r"(.*?)([ \t]*)(=)([ \t]*)").groups(&[T::NameAttribute, T::Text, T::Operator, T::Text]).push(&["value"]),
        ]),
        ("value", &[
            rule(r"-").token(T::Operator).pop(1),
            rule(r"(dword|hex(?:\([0-9a-fA-F]\))?)(:)([0-9a-fA-F,]+)").groups(&[T::NameVariable, T::Punctuation, T::LiteralNumber]).pop(1),
            rule(r".+").token(T::LiteralString).pop(1),
            rule("").pop(1),
        ]),
    ],
};
