//! Chroma's `bibtex.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "bibtex",
    config: ConfigDef {
        name: "BibTeX",
        aliases: &["bib", "bibtex"],
        filenames: &["*.bib"],
        mime_types: &["text/x-bibtex"],
        case_insensitive: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("closing-brace", &[
            include("whitespace"),
            rule(r"[})]").token(T::Punctuation).pop(1),
        ]),
        ("braced-string", &[
            rule(r"\{").token(T::LiteralString).push(&[]),
            rule(r"\}").token(T::LiteralString).pop(1),
            rule(r"[^\{\}]+").token(T::LiteralString),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::Text),
        ]),
        ("value", &[
            include("whitespace"),
            rule(r"[a-z_@!$&*+\-./:;<>?\[\\\]^`|~][\w@!$&*+\-./:;<>?\[\\\]^`|~]*").token(T::NameVariable),
            rule(r#"""#).token(T::LiteralString).push(&["quoted-string"]),
            rule(r"\{").token(T::LiteralString).push(&["braced-string"]),
            rule(r"[\d]+").token(T::LiteralNumber),
            rule(r"#").token(T::Punctuation),
            rule("").pop(1),
        ]),
        ("quoted-string", &[
            rule(r"\{").token(T::LiteralString).push(&["braced-string"]),
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"[^\{\"]+"#).token(T::LiteralString),
        ]),
        ("root", &[
            include("whitespace"),
            rule(r"@comment").token(T::Comment),
            rule(r"@preamble").token(T::NameClass).push(&["closing-brace", "value", "opening-brace"]),
            rule(r"@string").token(T::NameClass).push(&["closing-brace", "field", "opening-brace"]),
            rule(r"@[a-z_@!$&*+\-./:;<>?\[\\\]^`|~][\w@!$&*+\-./:;<>?\[\\\]^`|~]*").token(T::NameClass).push(&["closing-brace", "command-body", "opening-brace"]),
            rule(r".+").token(T::Comment),
        ]),
        ("command-body", &[
            include("whitespace"),
            rule(r"[^\s\,\}]+").token(T::NameLabel).push(&["#pop", "fields"]),
        ]),
        ("fields", &[
            include("whitespace"),
            rule(r",").token(T::Punctuation).push(&["field"]),
            rule("").pop(1),
        ]),
        ("=", &[
            include("whitespace"),
            rule(r"=").token(T::Punctuation).pop(1),
        ]),
        ("field", &[
            include("whitespace"),
            rule(r"[a-z_@!$&*+\-./:;<>?\[\\\]^`|~][\w@!$&*+\-./:;<>?\[\\\]^`|~]*").token(T::NameAttribute).push(&["value", "="]),
            rule("").pop(1),
        ]),
        ("opening-brace", &[
            include("whitespace"),
            rule(r"[{(]").token(T::Punctuation).pop(1),
        ]),
    ],
};
