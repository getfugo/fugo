//! Chroma's `tex.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "tex",
    config: ConfigDef {
        name: "TeX",
        aliases: &["tex", "latex"],
        filenames: &["*.tex", "*.aux", "*.toc"],
        mime_types: &["text/x-tex", "text/x-latex"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("displaymath", &[
            rule(r"\\\]").token(T::LiteralString).pop(1),
            rule(r"\$\$").token(T::LiteralString).pop(1),
            rule(r"\$").token(T::NameBuiltin),
            include("math"),
        ]),
        ("command", &[
            rule(r"\[.*?\]").token(T::NameAttribute),
            rule(r"\*").token(T::Keyword),
            rule("").pop(1),
        ]),
        ("general", &[
            rule(r"%.*?\n").token(T::Comment),
            rule(r"[{}]").token(T::NameBuiltin),
            rule(r"[&_^]").token(T::NameBuiltin),
        ]),
        ("root", &[
            rule(r"\\\[").token(T::LiteralStringBacktick).push(&["displaymath"]),
            rule(r"\\\(").token(T::LiteralString).push(&["inlinemath"]),
            rule(r"\$\$").token(T::LiteralStringBacktick).push(&["displaymath"]),
            rule(r"\$").token(T::LiteralString).push(&["inlinemath"]),
            rule(r"\\([a-zA-Z]+|.)").token(T::Keyword).push(&["command"]),
            rule(r"\\$").token(T::Keyword),
            include("general"),
            rule(r"[^\\$%&_^{}]+").token(T::Text),
        ]),
        ("math", &[
            rule(r"\\([a-zA-Z]+|.)").token(T::NameVariable),
            include("general"),
            rule(r"[0-9]+").token(T::LiteralNumber),
            rule(r"[-=!+*/()\[\]]").token(T::Operator),
            rule(r"[^=!+*/()\[\]\\$%&_^{}0-9-]+").token(T::NameBuiltin),
        ]),
        ("inlinemath", &[
            rule(r"\\\)").token(T::LiteralString).pop(1),
            rule(r"\$").token(T::LiteralString).pop(1),
            include("math"),
        ]),
    ],
};
