//! Chroma's `sed.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "sed",
    config: ConfigDef {
        name: "Sed",
        aliases: &["sed", "gsed", "ssed"],
        filenames: &["*.sed", "*.[gs]sed"],
        mime_types: &["text/x-sed"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"#.*$").token(T::CommentSingle),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"\$").token(T::Operator),
            rule(r"[{};,!]").token(T::Punctuation),
            rule(r"[dDFgGhHlnNpPqQxz=]").token(T::Keyword),
            rule(r"([berRtTvwW:])([^;\n]*)").groups(&[T::Keyword, T::LiteralStringSingle]),
            rule(r"([aci])((?:.*?\\\n)*(?:.*?[^\\]$))").groups(&[T::Keyword, T::LiteralStringDouble]),
            rule(r"([qQ])([0-9]*)").groups(&[T::Keyword, T::LiteralNumberInteger]),
            rule(r"(/)((?:(?:\\[^\n]|[^\\])*?\\\n)*?(?:\\.|[^\\])*?)(/)").groups(&[T::Punctuation, T::LiteralStringRegex, T::Punctuation]),
            rule(r"(\\(.))((?:(?:\\[^\n]|[^\\])*?\\\n)*?(?:\\.|[^\\])*?)(\2)").groups(&[T::Punctuation, T::Text, T::LiteralStringRegex, T::Punctuation]),
            rule(r"(y)(.)((?:(?:\\[^\n]|[^\\])*?\\\n)*?(?:\\.|[^\\])*?)(\2)((?:(?:\\[^\n]|[^\\])*?\\\n)*?(?:\\.|[^\\])*?)(\2)").groups(&[T::Keyword, T::Punctuation, T::LiteralStringSingle, T::Punctuation, T::LiteralStringSingle, T::Punctuation]),
            rule(r"(s)(.)((?:(?:\\[^\n]|[^\\])*?\\\n)*?(?:\\.|[^\\])*?)(\2)((?:(?:\\[^\n]|[^\\])*?\\\n)*?(?:\\.|[^\\])*?)(\2)((?:[gpeIiMm]|[0-9])*)").groups(&[T::Keyword, T::Punctuation, T::LiteralStringRegex, T::Punctuation, T::LiteralStringSingle, T::Punctuation, T::Keyword]),
        ]),
    ],
};
