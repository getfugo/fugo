//! Chroma's `nginx_configuration_file.xml` lexer, converted to Rust
//! (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "nginx_configuration_file",
    config: ConfigDef {
        name: "Nginx configuration file",
        aliases: &["nginx"],
        filenames: &["nginx.conf"],
        mime_types: &["text/x-nginx-conf"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(include)(\s+)([^\s;]+)").groups(&[T::Keyword, T::Text, T::Name]),
            rule(r"[^\s;#]+").token(T::Keyword).push(&["stmt"]),
            include("base"),
        ]),
        ("block", &[
            rule(r"\}").token(T::Punctuation).pop(2),
            rule(r"[^\s;#]+").token(T::KeywordNamespace).push(&["stmt"]),
            include("base"),
        ]),
        ("stmt", &[
            rule(r"\{").token(T::Punctuation).push(&["block"]),
            rule(r";").token(T::Punctuation).pop(1),
            include("base"),
        ]),
        ("base", &[
            rule(r"#.*\n").token(T::CommentSingle),
            rule(r"on|off").token(T::NameConstant),
            rule(r"\$[^\s;#()]+").token(T::NameVariable),
            rule(r"(\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b)").token(T::Name),
            rule(r"([a-z0-9.-]+)(:)([0-9]+)").groups(&[T::Name, T::Punctuation, T::LiteralNumberInteger]),
            rule(r"[a-z-]+/[a-z-+]+").token(T::LiteralString),
            rule(r"[0-9]+[km]?\b").token(T::LiteralNumberInteger),
            rule(r"(~)(\s*)([^\s{]+)").groups(&[T::Punctuation, T::Text, T::LiteralStringRegex]),
            rule(r"[:=~]").token(T::Punctuation),
            rule(r"[^\s;#{}$]+").token(T::LiteralString),
            rule(r"/[^\s;#]*").token(T::Name),
            rule(r"\s+").token(T::Text),
            rule(r"[$;]").token(T::Text),
        ]),
    ],
};
