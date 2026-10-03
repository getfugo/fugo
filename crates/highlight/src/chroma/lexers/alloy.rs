//! Chroma's `alloy.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "alloy",
    config: ConfigDef {
        name: "Alloy",
        aliases: &["alloy"],
        filenames: &["*.als"],
        mime_types: &["text/x-alloy"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("sig", &[
            rule(r"(extends)\b").token(T::Keyword).pop(1),
            rule(r#"[a-zA-Z_][\w]*"*"#).token(T::Name),
            rule(r"[^\S\n]+").token(T::TextWhitespace),
            rule(r",").token(T::Punctuation),
            rule(r"\{").token(T::Operator).pop(1),
        ]),
        ("module", &[
            rule(r"[^\S\n]+").token(T::TextWhitespace),
            rule(r#"[a-zA-Z_][\w]*"*"#).token(T::Name).pop(1),
        ]),
        ("fun", &[
            rule(r"[^\S\n]+").token(T::TextWhitespace),
            rule(r"\{").token(T::Operator).pop(1),
            rule(r#"[a-zA-Z_][\w]*"*"#).token(T::Name).pop(1),
        ]),
        ("fact", &[
            include("fun"),
            rule(r#""\b(\\\\|\\[^\\]|[^"\\])*""#).token(T::LiteralString).pop(1),
        ]),
        ("root", &[
            rule(r"--.*?$").token(T::CommentSingle),
            rule(r"//.*?$").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"[^\S\n]+").token(T::TextWhitespace),
            rule(r"(module|open)(\s+)").groups(&[T::KeywordNamespace, T::TextWhitespace]).push(&["module"]),
            rule(r"(sig|enum)(\s+)").groups(&[T::KeywordDeclaration, T::TextWhitespace]).push(&["sig"]),
            rule(r"(iden|univ|none)\b").token(T::KeywordConstant),
            rule(r"(int|Int)\b").token(T::KeywordType),
            rule(r"(var|this|abstract|extends|set|seq|one|lone|let)\b").token(T::Keyword),
            rule(r"(all|some|no|sum|disj|when|else)\b").token(T::Keyword),
            rule(r"(run|check|for|but|exactly|expect|as|steps)\b").token(T::Keyword),
            rule(r"(always|after|eventually|until|release)\b").token(T::Keyword),
            rule(r"(historically|before|once|since|triggered)\b").token(T::Keyword),
            rule(r"(and|or|implies|iff|in)\b").token(T::OperatorWord),
            rule(r"(fun|pred|assert)(\s+)").groups(&[T::Keyword, T::TextWhitespace]).push(&["fun"]),
            rule(r"(fact)(\s+)").groups(&[T::Keyword, T::TextWhitespace]).push(&["fact"]),
            rule(r"!|#|&&|\+\+|<<|>>|>=|<=>|<=|\.\.|\.|->").token(T::Operator),
            rule(r"[-+/*%=<>&!^|~{}\[\]().\';]").token(T::Operator),
            rule(r#"[a-zA-Z_][\w]*"*"#).token(T::Name),
            rule(r"[:,]").token(T::Punctuation),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r#""\b(\\\\|\\[^\\]|[^"\\])*""#).token(T::LiteralString),
            rule(r"\n").token(T::TextWhitespace),
        ]),
    ],
};
