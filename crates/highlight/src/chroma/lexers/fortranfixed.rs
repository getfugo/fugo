//! Chroma's `fortranfixed.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "fortranfixed",
    config: ConfigDef {
        name: "FortranFixed",
        aliases: &["fortranfixed"],
        filenames: &["*.f", "*.F"],
        mime_types: &["text/x-fortran"],
        case_insensitive: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("cont-char", &[
            rule(r" ").token(T::TextWhitespace).push(&["code"]),
            rule(r".").token(T::GenericStrong).push(&["code"]),
        ]),
        ("code", &[
            rule(r"(.{66})(.*)(\n)").bygroups(&[E::Using("Fortran"), E::Token(T::Comment), E::Token(T::TextWhitespace)]).push(&["root"]),
            rule(r"(.*)(!.*)(\n)").bygroups(&[E::Using("Fortran"), E::Token(T::Comment), E::Token(T::TextWhitespace)]).push(&["root"]),
            rule(r"(.*)(\n)").bygroups(&[E::Using("Fortran"), E::Token(T::TextWhitespace)]).push(&["root"]),
            rule("").mutators(&[M::Push(&["root"])]),
        ]),
        ("root", &[
            rule(r"[C*].*\n").token(T::Comment),
            rule(r"#.*\n").token(T::CommentPreproc),
            rule(r" {0,4}!.*\n").token(T::Comment),
            rule(r"(.{5})").token(T::NameLabel).push(&["cont-char"]),
            rule(r".*\n").using("Fortran"),
        ]),
    ],
};
