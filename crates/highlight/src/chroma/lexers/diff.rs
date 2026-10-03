//! Chroma's `diff.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "diff",
    config: ConfigDef {
        name: "Diff",
        aliases: &["diff", "udiff"],
        filenames: &["*.diff", "*.patch"],
        mime_types: &["text/x-diff", "text/x-patch"],
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r" .*\n").token(T::Text),
            rule(r"\d+(,\d+)?(a|c|d)\d+(,\d+)?\n").token(T::GenericSubheading),
            rule(r"---\n").token(T::GenericStrong),
            rule(r"< .*\n").token(T::GenericDeleted),
            rule(r"> .*\n").token(T::GenericInserted),
            rule(r"\+.*\n").token(T::GenericInserted),
            rule(r"-.*\n").token(T::GenericDeleted),
            rule(r"!.*\n").token(T::GenericStrong),
            rule(r"@.*\n").token(T::GenericSubheading),
            rule(r"([Ii]ndex|diff).*\n").token(T::GenericHeading),
            rule(r"=.*\n").token(T::GenericHeading),
            rule(r".*\n").token(T::Text),
        ]),
    ],
};
