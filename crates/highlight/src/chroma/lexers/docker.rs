//! Chroma's `docker.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "docker",
    config: ConfigDef {
        name: "Docker",
        aliases: &["docker", "dockerfile"],
        filenames: &["Dockerfile", "Dockerfile.*", "*.Dockerfile", "*.docker"],
        mime_types: &["text/x-dockerfile-config"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"#.*").token(T::Comment),
            rule(r"(ONBUILD)((?:\s*\\?\s*))").bygroups(&[E::Token(T::Keyword), E::Using("Bash")]),
            rule(r"(HEALTHCHECK)((?:(?:\s*\\?\s*)--\w+=\w+(?:\s*\\?\s*))*)").bygroups(&[E::Token(T::Keyword), E::Using("Bash")]),
            rule(r"(VOLUME|ENTRYPOINT|CMD|SHELL)((?:\s*\\?\s*))(\[.*?\])").bygroups(&[E::Token(T::Keyword), E::Using("Bash"), E::Using("JSON")]),
            rule(r"(LABEL|ENV|ARG)((?:(?:\s*\\?\s*)\w+=\w+(?:\s*\\?\s*))*)").bygroups(&[E::Token(T::Keyword), E::Using("Bash")]),
            rule(r"((?:FROM|MAINTAINER|EXPOSE|WORKDIR|USER|STOPSIGNAL)|VOLUME)\b(.*)").groups(&[T::Keyword, T::LiteralString]),
            rule(r"((?:RUN|CMD|ENTRYPOINT|ENV|ARG|LABEL|ADD|COPY))").token(T::Keyword),
            rule(r"(.*\\\n)*.+").using("Bash"),
        ]),
    ],
};
