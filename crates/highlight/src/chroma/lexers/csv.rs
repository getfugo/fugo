//! Chroma's `csv.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

// Lexer for RFC-4180 compliant CSV subject to the following additions:
// - UTF-8 encoding is accepted (the RFC requires 7-bit ASCII)
// - The line terminator character can be LF or CRLF (the RFC allows CRLF only)
//
// Link to the RFC-4180 specification: https://tools.ietf.org/html/rfc4180
//
// Additions inspired by:
// https://github.com/frictionlessdata/datapackage/issues/204#issuecomment-193242077
//
// Future improvements:
// - Identify non-quoted numbers as LiteralNumber
// - Identify y as an error in "x"y. Currently it's identified as another string
//   literal.
#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "csv",
    config: ConfigDef {
        name: "CSV",
        aliases: &["csv"],
        filenames: &["*.csv"],
        mime_types: &["text/csv"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\r?\n").token(T::Punctuation),
            rule(r",").token(T::Punctuation),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["escaped"]),
            rule(r"[^\r\n,]+").token(T::LiteralString),
        ]),
        ("escaped", &[
            rule(r#""""#).token(T::LiteralStringEscape),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"[^"]+"#).token(T::LiteralStringDouble),
        ]),
    ],
};
