//! Chroma's `abnf.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "abnf",
    config: ConfigDef {
        name: "ABNF",
        aliases: &["abnf"],
        filenames: &["*.abnf"],
        mime_types: &["text/x-abnf"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r";.*$").token(T::CommentSingle),
            rule(r#"(%[si])?"[^"]*""#).token(T::Literal),
            rule(r"%b[01]+\-[01]+\b").token(T::Literal),
            rule(r"%b[01]+(\.[01]+)*\b").token(T::Literal),
            rule(r"%d[0-9]+\-[0-9]+\b").token(T::Literal),
            rule(r"%d[0-9]+(\.[0-9]+)*\b").token(T::Literal),
            rule(r"%x[0-9a-fA-F]+\-[0-9a-fA-F]+\b").token(T::Literal),
            rule(r"%x[0-9a-fA-F]+(\.[0-9a-fA-F]+)*\b").token(T::Literal),
            rule(r"\b[0-9]+\*[0-9]+").token(T::Operator),
            rule(r"\b[0-9]+\*").token(T::Operator),
            rule(r"\b[0-9]+").token(T::Operator),
            rule(r"\*").token(T::Operator),
            rule(r"(HEXDIG|DQUOTE|DIGIT|VCHAR|OCTET|ALPHA|CHAR|CRLF|HTAB|LWSP|BIT|CTL|WSP|LF|SP|CR)\b").token(T::Keyword),
            rule(r"[a-zA-Z][a-zA-Z0-9-]+\b").token(T::NameClass),
            rule(r"(=/|=|/)").token(T::Operator),
            rule(r"[\[\]()]").token(T::Punctuation),
            rule(r"\s+").token(T::Text),
            rule(r".").token(T::Text),
        ]),
    ],
};
