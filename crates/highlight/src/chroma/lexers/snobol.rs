//! Chroma's `snobol.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "snobol",
    config: ConfigDef {
        name: "Snobol",
        aliases: &["snobol"],
        filenames: &["*.snobol"],
        mime_types: &["text/x-snobol"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("heredoc", &[
            rule(r".*\n").token(T::LiteralStringHeredoc),
        ]),
        ("root", &[
            rule(r"\*.*\n").token(T::Comment),
            rule(r"[+.] ").token(T::Punctuation).push(&["statement"]),
            rule(r"-.*\n").token(T::Comment),
            rule(r"END\s*\n").token(T::NameLabel).push(&["heredoc"]),
            rule(r"[A-Za-z$][\w$]*").token(T::NameLabel).push(&["statement"]),
            rule(r"\s+").token(T::Text).push(&["statement"]),
        ]),
        ("statement", &[
            rule(r"\s*\n").token(T::Text).pop(1),
            rule(r"\s+").token(T::Text),
            rule(r"(?<=[^\w.])(LT|LE|EQ|NE|GE|GT|INTEGER|IDENT|DIFFER|LGT|SIZE|REPLACE|TRIM|DUPL|REMDR|DATE|TIME|EVAL|APPLY|OPSYN|LOAD|UNLOAD|LEN|SPAN|BREAK|ANY|NOTANY|TAB|RTAB|REM|POS|RPOS|FAIL|FENCE|ABORT|ARB|ARBNO|BAL|SUCCEED|INPUT|OUTPUT|TERMINAL)(?=[^\w.])").token(T::NameBuiltin),
            rule(r"[A-Za-z][\w.]*").token(T::Name),
            rule(r"\*\*|[?$.!%*/#+\-@|&\\=]").token(T::Operator),
            rule(r#""[^"]*""#).token(T::LiteralString),
            rule(r"'[^']*'").token(T::LiteralString),
            rule(r"[0-9]+(?=[^.EeDd])").token(T::LiteralNumberInteger),
            rule(r"[0-9]+(\.[0-9]*)?([EDed][-+]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r":").token(T::Punctuation).push(&["goto"]),
            rule(r"[()<>,;]").token(T::Punctuation),
        ]),
        ("goto", &[
            rule(r"\s*\n").token(T::Text).pop(2),
            rule(r"\s+").token(T::Text),
            rule(r"F|S").token(T::Keyword),
            rule(r"(\()([A-Za-z][\w.]*)(\))").groups(&[T::Punctuation, T::NameLabel, T::Punctuation]),
        ]),
    ],
};
