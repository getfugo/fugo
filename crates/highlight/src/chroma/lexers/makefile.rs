//! Chroma's `makefile.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "makefile",
    config: ConfigDef {
        name: "Makefile",
        aliases: &["make", "makefile", "mf", "bsdmake"],
        filenames: &[
            "*.mak",
            "*.mk",
            "Makefile",
            "makefile",
            "Makefile.*",
            "GNUmakefile",
            "BSDmakefile",
            "Justfile",
            "justfile",
            ".justfile",
        ],
        mime_types: &["text/x-makefile"],
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^(?:[\t ]+.*\n|\n)+").using("Bash"),
            rule(r"\$[<@$+%?|*]").token(T::Keyword),
            rule(r"\s+").token(T::Text),
            rule(r"#.*?\n").token(T::Comment),
            rule(r"(export)(\s+)(?=[\w${}\t -]+\n)").groups(&[T::Keyword, T::Text]).push(&["export"]),
            rule(r"export\s+").token(T::Keyword),
            rule(r"([\w${}().-]+)(\s*)([!?:+]?=)([ \t]*)((?:.*\\\n)+|.*\n)").bygroups(&[E::Token(T::NameVariable), E::Token(T::Text), E::Token(T::Operator), E::Token(T::Text), E::Using("Bash")]),
            rule(r#"(?s)"(\\\\|\\.|[^"\\])*""#).token(T::LiteralStringDouble),
            rule(r"(?s)'(\\\\|\\.|[^'\\])*'").token(T::LiteralStringSingle),
            rule(r"([^\n:]+)(:+)([ \t]*)").groups(&[T::NameFunction, T::Operator, T::Text]).push(&["block-header"]),
            rule(r"\$\(").token(T::Keyword).push(&["expansion"]),
        ]),
        ("expansion", &[
            rule(r"[^$a-zA-Z_()]+").token(T::Text),
            rule(r"[a-zA-Z_]+").token(T::NameVariable),
            rule(r"\$").token(T::Keyword),
            rule(r"\(").token(T::Keyword).push(&[]),
            rule(r"\)").token(T::Keyword).pop(1),
        ]),
        ("export", &[
            rule(r"[\w${}-]+").token(T::NameVariable),
            rule(r"\n").token(T::Text).pop(1),
            rule(r"\s+").token(T::Text),
        ]),
        ("block-header", &[
            rule(r"[,|]").token(T::Punctuation),
            rule(r"#.*?\n").token(T::Comment).pop(1),
            rule(r"\\\n").token(T::Text),
            rule(r"\$\(").token(T::Keyword).push(&["expansion"]),
            rule(r"[a-zA-Z_]+").token(T::Name),
            rule(r"\n").token(T::Text).pop(1),
            rule(r".").token(T::Text),
        ]),
    ],
};
