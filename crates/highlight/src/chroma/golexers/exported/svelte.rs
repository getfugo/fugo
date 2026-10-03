//! Chroma's `svelte.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "svelte",
    config: ConfigDef {
        name: "Svelte",
        aliases: &["svelte"],
        filenames: &["*.svelte"],
        mime_types: &["application/x-svelte"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("comment", &[
            rule(r"-->").token(T::Other).pop(1),
            rule(r".+?").token(T::Other),
        ]),
        ("root", &[
            rule(r"<!--").token(T::Other).push(&["comment"]),
            rule(r#"(<\s*(?:script|style).*?lang\s*=\s*['"])(.+?)(['"].*?>)(.+?)(<\s*/\s*(?:script|style)\s*>)"#).using_by_group(2, 4, &[E::Token(T::Other), E::Token(T::Other), E::Token(T::Other), E::Token(T::Other), E::Token(T::Other)]),
            rule(r"(?<!<\s*(?:script|style)(?:(?!(?:script|style)\s*>).)*?){(?!(?:(?!<\s*(?:script|style)).)*?(?:script|style)\s*>)").token(T::Punctuation).push(&["templates"]),
            rule(r"(?<=\s+on:\w+(?:\|\w+)*)\|(?=\w+)").token(T::Operator),
            rule(r".+?").token(T::Other),
        ]),
        ("templates", &[
            rule(r"}").token(T::Punctuation).pop(1),
            rule(r#"(?<!(?<!\\)\\)(['"`]).*?(?<!(?<!\\)\\)\1"#).using("TypeScript"),
            rule(r"{").token(T::Punctuation).push(&["templates"]),
            rule(r"@(debug|html)\b").token(T::Keyword),
            rule(r"(#await)(\s+)(\w+)(\s+)(then|catch)(\s+)(\w+)").bygroups(&[E::Token(T::Keyword), E::Token(T::Text), E::Using("TypeScript"), E::Token(T::Text), E::Token(T::Keyword), E::Token(T::Text), E::Using("TypeScript")]),
            rule(r"(#|/)(await|each|if|key)\b").token(T::Keyword),
            rule(r"(:else)(\s+)(if)?\b").groups(&[T::Keyword, T::Text, T::Keyword]),
            rule(r":(catch|then)\b").token(T::Keyword),
            rule(r"[^{}]+").using("TypeScript"),
        ]),
    ],
};
