//! Chroma's `mathematica.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "mathematica",
    config: ConfigDef {
        name: "Mathematica",
        aliases: &["mathematica", "mma", "nb"],
        filenames: &["*.cdf", "*.m", "*.ma", "*.mt", "*.mx", "*.nb", "*.nbp", "*.wl"],
        mime_types: &[
            "application/mathematica",
            "application/vnd.wolfram.mathematica",
            "application/vnd.wolfram.mathematica.package",
            "application/vnd.wolfram.cdf",
        ],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(?s)\(\*.*?\*\)").token(T::Comment),
            rule(r"([a-zA-Z]+[A-Za-z0-9]*`)").token(T::NameNamespace),
            rule(r"([A-Za-z0-9]*_+[A-Za-z0-9]*)").token(T::NameVariable),
            rule(r"#\d*").token(T::NameVariable),
            rule(r"([a-zA-Z]+[a-zA-Z0-9]*)").token(T::Name),
            rule(r"-?\d+\.\d*").token(T::LiteralNumberFloat),
            rule(r"-?\d*\.\d+").token(T::LiteralNumberFloat),
            rule(r"-?\d+").token(T::LiteralNumberInteger),
            rule(r"(!===|@@@|===|/;|:=|->|:>|/\.|=\.|~~|<=|@@|/@|&&|\|\||//|<>|;;|>=|-|@|!|\^|/|\*|\?|\+|&|<|>|=|\|)").token(T::Operator),
            rule(r"(,|;|\(|\)|\[|\]|\{|\})").token(T::Punctuation),
            rule(r#"".*?""#).token(T::LiteralString),
            rule(r"\s+").token(T::TextWhitespace),
        ]),
    ],
};
