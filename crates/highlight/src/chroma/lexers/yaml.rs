//! Chroma's `yaml.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "yaml",
    config: ConfigDef {
        name: "YAML",
        aliases: &["yaml"],
        filenames: &["*.yaml", "*.yml"],
        mime_types: &["text/x-yaml"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            include("whitespace"),
            rule(r"^---").token(T::NameNamespace),
            rule(r"^\.\.\.").token(T::NameNamespace),
            rule(r"[\n?]?\s*- ").token(T::Text),
            rule(r"#.*$").token(T::Comment),
            rule(r"!![^\s]+").token(T::CommentPreproc),
            rule(r"&[^\s]+").token(T::CommentPreproc),
            rule(r"\*[^\s]+").token(T::CommentPreproc),
            rule(r"^%include\s+[^\n\r]+").token(T::CommentPreproc),
            include("key"),
            include("value"),
            rule(r"[?:,\[\]]").token(T::Punctuation),
            rule(r".").token(T::Text),
        ]),
        ("value", &[
            rule(r"([>|](?:[+-])?)(\n(^ {1,})(?:(?:.*\n*(?:^\3 *).*)+|.*))").groups(&[T::Punctuation, T::LiteralStringDoc, T::Ignore]),
            rule(r"(false|False|FALSE|true|True|TRUE|null|Off|off|yes|Yes|YES|OFF|On|ON|no|No|on|NO|n|N|Y|y)\b").token(T::KeywordConstant),
            rule(r#""(?:\\.|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(?:\\.|[^'])*'").token(T::LiteralStringSingle),
            rule(r"\d\d\d\d-\d\d-\d\d([T ]\d\d:\d\d:\d\d(\.\d+)?(Z|\s+[-+]\d+)?)?").token(T::LiteralDate),
            rule(r"\b[+\-]?(0x[\da-f]+|0o[0-7]+|(\d+\.?\d*|\.?\d+)(e[\+\-]?\d+)?|\.inf|\.nan)\b").token(T::LiteralNumber),
            rule(r"([^\{\}\[\]\?,\:\!\-\*&\@].*)( )+(#.*)").groups(&[T::Literal, T::TextWhitespace, T::Comment]),
            rule(r"[^\{\}\[\]\?,\:\!\-\*&\@].*").token(T::Literal),
        ]),
        ("key", &[
            rule(r#""[^"\n].*": "#).token(T::NameTag),
            rule(r#"(-)( )([^"\n{]*)(:)( )"#).groups(&[T::Punctuation, T::TextWhitespace, T::NameTag, T::Punctuation, T::TextWhitespace]),
            rule(r#"([^"\n{]*)(:)( )"#).groups(&[T::NameTag, T::Punctuation, T::TextWhitespace]),
            rule(r#"([^"\n{]*)(:)(\n)"#).groups(&[T::NameTag, T::Punctuation, T::TextWhitespace]),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"\n+").token(T::TextWhitespace),
        ]),
    ],
};
