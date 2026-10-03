//! Chroma's `bash_session.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "bash_session",
    config: ConfigDef {
        name: "Bash Session",
        aliases: &["bash-session", "console", "shell-session"],
        filenames: &["*.sh-session"],
        mime_types: &["text/x-sh"],
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^((?:\[[^]]+@[^]]+\]\s?)?[#$%>])(\s*)(.*\n?)").bygroups(&[E::Token(T::GenericPrompt), E::Token(T::Text), E::Using("bash")]),
            rule(r"^.+\n?").token(T::GenericOutput),
        ]),
    ],
};
