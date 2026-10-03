//! Chroma's `hcl.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "hcl",
    config: ConfigDef {
        name: "HCL",
        aliases: &["hcl"],
        filenames: &["*.hcl"],
        mime_types: &["application/x-hcl"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("punctuation", &[
            rule(r"[\[\](),.]").token(T::Punctuation),
        ]),
        ("string", &[
            rule(r#"(".*")"#).groups(&[T::LiteralStringDouble]),
        ]),
        ("whitespace", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
        ]),
        ("root", &[
            include("string"),
            include("punctuation"),
            include("curly"),
            include("basic"),
            include("whitespace"),
            rule(r"[0-9]+").token(T::LiteralNumber),
        ]),
        ("basic", &[
            rule(r"\b(false|true)\b").token(T::KeywordType),
            rule(r"\s*/\*").token(T::CommentMultiline).push(&["comment"]),
            rule(r"\s*#.*\n").token(T::CommentSingle),
            rule(r"(.*?)(\s*)(=)").groups(&[T::Name, T::Text, T::Operator]),
            rule(r"\d+").token(T::LiteralNumber),
            rule(r"\b\w+\b").token(T::Keyword),
            rule(r"\$\{").token(T::LiteralStringInterpol).push(&["var_builtin"]),
        ]),
        ("curly", &[
            rule(r"\{").token(T::TextPunctuation),
            rule(r"\}").token(T::TextPunctuation),
        ]),
        ("function", &[
            rule(r#"(\s+)(".*")(\s+)"#).groups(&[T::Text, T::LiteralString, T::Text]),
            include("punctuation"),
            include("curly"),
        ]),
        ("var_builtin", &[
            rule(r"\$\{").token(T::LiteralStringInterpol).push(&[]),
            rule(r"\b(element|concat|lookup|file|join)\b").token(T::NameBuiltin),
            include("string"),
            include("punctuation"),
            rule(r"\s+").token(T::Text),
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
        ]),
        ("comment", &[
            rule(r"[^*/]").token(T::CommentMultiline),
            rule(r"/\*").token(T::CommentMultiline).push(&[]),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"[*/]").token(T::CommentMultiline),
        ]),
    ],
};
