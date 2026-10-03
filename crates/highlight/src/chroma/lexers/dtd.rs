//! Chroma's `dtd.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "dtd",
    config: ConfigDef {
        name: "DTD",
        aliases: &["dtd"],
        filenames: &["*.dtd"],
        mime_types: &["application/xml-dtd"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("common", &[
            rule(r"\s+").token(T::Text),
            rule(r"(%|&)[^;]*;").token(T::NameEntity),
            rule(r"<!--").token(T::Comment).push(&["comment"]),
            rule(r"[(|)*,?+]").token(T::Operator),
            rule(r#""[^"]*""#).token(T::LiteralStringDouble),
            rule(r"\'[^\']*\'").token(T::LiteralStringSingle),
        ]),
        ("comment", &[
            rule(r"[^-]+").token(T::Comment),
            rule(r"-->").token(T::Comment).pop(1),
            rule(r"-").token(T::Comment),
        ]),
        ("element", &[
            include("common"),
            rule(r"EMPTY|ANY|#PCDATA").token(T::KeywordConstant),
            rule(r"[^>\s|()?+*,]+").token(T::NameTag),
            rule(r">").token(T::Keyword).pop(1),
        ]),
        ("attlist", &[
            include("common"),
            rule(r"CDATA|IDREFS|IDREF|ID|NMTOKENS|NMTOKEN|ENTITIES|ENTITY|NOTATION").token(T::KeywordConstant),
            rule(r"#REQUIRED|#IMPLIED|#FIXED").token(T::KeywordConstant),
            rule(r"xml:space|xml:lang").token(T::KeywordReserved),
            rule(r"[^>\s|()?+*,]+").token(T::NameAttribute),
            rule(r">").token(T::Keyword).pop(1),
        ]),
        ("entity", &[
            include("common"),
            rule(r"SYSTEM|PUBLIC|NDATA").token(T::KeywordConstant),
            rule(r"[^>\s|()?+*,]+").token(T::NameEntity),
            rule(r">").token(T::Keyword).pop(1),
        ]),
        ("notation", &[
            include("common"),
            rule(r"SYSTEM|PUBLIC").token(T::KeywordConstant),
            rule(r"[^>\s|()?+*,]+").token(T::NameAttribute),
            rule(r">").token(T::Keyword).pop(1),
        ]),
        ("root", &[
            include("common"),
            rule(r"(<!ELEMENT)(\s+)(\S+)").groups(&[T::Keyword, T::Text, T::NameTag]).push(&["element"]),
            rule(r"(<!ATTLIST)(\s+)(\S+)").groups(&[T::Keyword, T::Text, T::NameTag]).push(&["attlist"]),
            rule(r"(<!ENTITY)(\s+)(\S+)").groups(&[T::Keyword, T::Text, T::NameEntity]).push(&["entity"]),
            rule(r"(<!NOTATION)(\s+)(\S+)").groups(&[T::Keyword, T::Text, T::NameTag]).push(&["notation"]),
            rule(r"(<!\[)([^\[\s]+)(\s*)(\[)").groups(&[T::Keyword, T::NameEntity, T::Text, T::Keyword]),
            rule(r"(<!DOCTYPE)(\s+)([^>\s]+)").groups(&[T::Keyword, T::Text, T::NameTag]),
            rule(r"PUBLIC|SYSTEM").token(T::KeywordConstant),
            rule(r"[\[\]>]").token(T::Keyword),
        ]),
    ],
};
