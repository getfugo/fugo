//! Chroma's `armasm.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "armasm",
    config: ConfigDef {
        name: "ArmAsm",
        aliases: &["armasm"],
        filenames: &["*.s", "*.S"],
        mime_types: &["text/x-armasm", "text/x-asm"],
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            include("commentsandwhitespace"),
            rule(r"(\.\w+)([ \t]+\w+\s+?)?").groups(&[T::KeywordNamespace, T::NameLabel]),
            rule(r"(\w+)(:)(\s+\.\w+\s+)").groups(&[T::NameLabel, T::Punctuation, T::KeywordNamespace]).push(&["literal"]),
            rule(r"(\w+)(:)").groups(&[T::NameLabel, T::Punctuation]),
            rule(r"svc\s+\w+").token(T::NameNamespace),
            rule(r"[a-zA-Z]+").token(T::Text).push(&["opcode"]),
        ]),
        ("commentsandwhitespace", &[
            rule(r"\s+").token(T::Text),
            rule(r"[@;].*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
        ]),
        ("literal", &[
            rule(r"0b[01]+").token(T::LiteralNumberBin).pop(1),
            rule(r"0x\w{1,8}").token(T::LiteralNumberHex).pop(1),
            rule(r"0\d+").token(T::LiteralNumberOct).pop(1),
            rule(r"\d+?\.\d+?").token(T::LiteralNumberFloat).pop(1),
            rule(r"\d+").token(T::LiteralNumberInteger).pop(1),
            rule(r#"(")(.+)(")"#).groups(&[T::Punctuation, T::LiteralStringDouble, T::Punctuation]).pop(1),
            rule(r"(')(.{1}|\\.{1})(')").groups(&[T::Punctuation, T::LiteralStringChar, T::Punctuation]).pop(1),
        ]),
        ("opcode", &[
            rule(r"\n").token(T::Text).pop(1),
            rule(r"(@|;).*\n").token(T::CommentSingle).pop(1),
            rule(r"(\s+|,)").token(T::Text),
            rule(r"[rapcfxwbhsdqv]\d{1,2}").token(T::NameClass),
            rule(r"(=)(0x\w+)").groups(&[T::Text, T::NameLabel]),
            rule(r"(=)(\w+)").groups(&[T::Text, T::NameLabel]),
            rule(r"#").token(T::Text).push(&["literal"]),
        ]),
    ],
};
