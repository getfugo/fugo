//! Chroma's `html.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "html",
    config: ConfigDef {
        name: "HTML",
        aliases: &["html"],
        filenames: &["*.html", "*.htm", "*.xhtml", "*.xslt"],
        mime_types: &["text/html", "application/xhtml+xml"],
        case_insensitive: true,
        dot_all: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("script-content", &[
            rule(r"(<)(\s*)(/)(\s*)(script)(\s*)(>)").groups(&[T::Punctuation, T::Text, T::Punctuation, T::Text, T::NameTag, T::Text, T::Punctuation]).pop(1),
            rule(r".+?(?=<\s*/\s*script\s*>)").using("Javascript"),
        ]),
        ("style-content", &[
            rule(r"(<)(\s*)(/)(\s*)(style)(\s*)(>)").groups(&[T::Punctuation, T::Text, T::Punctuation, T::Text, T::NameTag, T::Text, T::Punctuation]).pop(1),
            rule(r".+?(?=<\s*/\s*style\s*>)").using("CSS"),
        ]),
        ("attr", &[
            rule(r#"".*?""#).token(T::LiteralString).pop(1),
            rule(r"'.*?'").token(T::LiteralString).pop(1),
            rule(r"[^\s>]+").token(T::LiteralString).pop(1),
        ]),
        ("root", &[
            rule(r"[^<&]+").token(T::Text),
            rule(r"&\S*?;").token(T::NameEntity),
            rule(r"\<\!\[CDATA\[.*?\]\]\>").token(T::CommentPreproc),
            rule(r"<!--").token(T::Comment).push(&["comment"]),
            rule(r"<\?.*?\?>").token(T::CommentPreproc),
            rule(r"<![^>]*>").token(T::CommentPreproc),
            rule(r"(<)(\s*)(script)(\s*)").groups(&[T::Punctuation, T::Text, T::NameTag, T::Text]).push(&["script-content", "tag"]),
            rule(r"(<)(\s*)(style)(\s*)").groups(&[T::Punctuation, T::Text, T::NameTag, T::Text]).push(&["style-content", "tag"]),
            rule(r"(<)(\s*)([\w:.-]+)").groups(&[T::Punctuation, T::Text, T::NameTag]).push(&["tag"]),
            rule(r"(<)(\s*)(/)(\s*)([\w:.-]+)(\s*)(>)").groups(&[T::Punctuation, T::Text, T::Punctuation, T::Text, T::NameTag, T::Text, T::Punctuation]),
        ]),
        ("comment", &[
            rule(r"[^-]+").token(T::Comment),
            rule(r"-->").token(T::Comment).pop(1),
            rule(r"-").token(T::Comment),
        ]),
        ("tag", &[
            rule(r"\s+").token(T::Text),
            rule(r"([\w:-]+\s*)(=)(\s*)").groups(&[T::NameAttribute, T::Operator, T::Text]).push(&["attr"]),
            rule(r"[\w:-]+").token(T::NameAttribute),
            rule(r"(/?)(\s*)(>)").groups(&[T::Punctuation, T::Text, T::Punctuation]).pop(1),
        ]),
    ],
};
