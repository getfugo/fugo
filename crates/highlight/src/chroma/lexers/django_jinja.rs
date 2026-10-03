//! Chroma's `django_jinja.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "django_jinja",
    config: ConfigDef {
        name: "Django/Jinja",
        aliases: &["django", "jinja"],
        mime_types: &["application/x-django-templating", "application/x-jinja"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("var", &[
            rule(r"\s+").token(T::Text),
            rule(r"(-?)(\}\})").groups(&[T::Text, T::CommentPreproc]).pop(1),
            include("varnames"),
        ]),
        ("block", &[
            rule(r"\s+").token(T::Text),
            rule(r"(-?)(%\})").groups(&[T::Text, T::CommentPreproc]).pop(1),
            include("varnames"),
            rule(r".").token(T::Punctuation),
        ]),
        ("root", &[
            rule(r"[^{]+").token(T::Other),
            rule(r"\{\{").token(T::CommentPreproc).push(&["var"]),
            rule(r"\{[*#].*?[*#]\}").token(T::Comment),
            rule(r"(\{%)(-?\s*)(comment)(\s*-?)(%\})(.*?)(\{%)(-?\s*)(endcomment)(\s*-?)(%\})").groups(&[T::CommentPreproc, T::Text, T::Keyword, T::Text, T::CommentPreproc, T::Comment, T::CommentPreproc, T::Text, T::Keyword, T::Text, T::CommentPreproc]),
            rule(r"(\{%)(-?\s*)(raw)(\s*-?)(%\})(.*?)(\{%)(-?\s*)(endraw)(\s*-?)(%\})").groups(&[T::CommentPreproc, T::Text, T::Keyword, T::Text, T::CommentPreproc, T::Text, T::CommentPreproc, T::Text, T::Keyword, T::Text, T::CommentPreproc]),
            rule(r"(\{%)(-?\s*)(filter)(\s+)([a-zA-Z_]\w*)").groups(&[T::CommentPreproc, T::Text, T::Keyword, T::Text, T::NameFunction]).push(&["block"]),
            rule(r"(\{%)(-?\s*)([a-zA-Z_]\w*)").groups(&[T::CommentPreproc, T::Text, T::Keyword]).push(&["block"]),
            rule(r"\{").token(T::Other),
        ]),
        ("varnames", &[
            rule(r"(\|)(\s*)([a-zA-Z_]\w*)").groups(&[T::Operator, T::Text, T::NameFunction]),
            rule(r"(is)(\s+)(not)?(\s+)?([a-zA-Z_]\w*)").groups(&[T::Keyword, T::Text, T::Keyword, T::Text, T::NameFunction]),
            rule(r"(_|true|false|none|True|False|None)\b").token(T::KeywordPseudo),
            rule(r"(in|as|reversed|recursive|not|and|or|is|if|else|import|with(?:(?:out)?\s*context)?|scoped|ignore\s+missing)\b").token(T::Keyword),
            rule(r"(loop|block|super|forloop)\b").token(T::NameBuiltin),
            rule(r"[a-zA-Z_][\w-]*").token(T::NameVariable),
            rule(r"\.\w+").token(T::NameVariable),
            rule(r#":?"(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r":?'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"([{}()\[\]+\-*/,:~]|[><=]=?)").token(T::Operator),
            rule(r"[0-9](\.[0-9]*)?(eE[+-][0-9])?[flFLdD]?|0[xX][0-9a-fA-F]+[Ll]?").token(T::LiteralNumber),
        ]),
    ],
};
