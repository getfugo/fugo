//! Chroma's `xml.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "xml",
    config: ConfigDef {
        name: "XML",
        aliases: &["xml"],
        filenames: &[
            "*.xml",
            "*.xsl",
            "*.rss",
            "*.xslt",
            "*.xsd",
            "*.wsdl",
            "*.wsf",
            "*.svg",
            "*.csproj",
            "*.vcxproj",
            "*.fsproj",
        ],
        mime_types: &[
            "text/xml",
            "application/xml",
            "image/svg+xml",
            "application/rss+xml",
            "application/atom+xml",
        ],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"[^<&]+").token(T::Text),
            rule(r"&\S*?;").token(T::NameEntity),
            rule(r"\<\!\[CDATA\[.*?\]\]\>").token(T::CommentPreproc),
            rule(r"<!--").token(T::Comment).push(&["comment"]),
            rule(r"<\?.*?\?>").token(T::CommentPreproc),
            rule(r"<![^>]*>").token(T::CommentPreproc),
            rule(r"<\s*[\w:.-]+").token(T::NameTag).push(&["tag"]),
            rule(r"<\s*/\s*[\w:.-]+\s*>").token(T::NameTag),
        ]),
        ("comment", &[
            rule(r"[^-]+").token(T::Comment),
            rule(r"-->").token(T::Comment).pop(1),
            rule(r"-").token(T::Comment),
        ]),
        ("tag", &[
            rule(r"\s+").token(T::Text),
            rule(r"[\w.:-]+\s*=").token(T::NameAttribute).push(&["attr"]),
            rule(r"/?\s*>").token(T::NameTag).pop(1),
        ]),
        ("attr", &[
            rule(r"\s+").token(T::Text),
            rule(r#"".*?""#).token(T::LiteralString).pop(1),
            rule(r"'.*?'").token(T::LiteralString).pop(1),
            rule(r"[^\s>]+").token(T::LiteralString).pop(1),
        ]),
    ],
};
