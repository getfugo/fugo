//! Chroma's `elm.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "elm",
    config: ConfigDef {
        name: "Elm",
        aliases: &["elm"],
        filenames: &["*.elm"],
        mime_types: &["text/x-elm"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("shader", &[
            rule(r"\|(?!\])").token(T::NameEntity),
            rule(r"\|\]").token(T::NameEntity).pop(1),
            rule(r".*\n").token(T::NameEntity),
        ]),
        ("root", &[
            rule(r"\{-").token(T::CommentMultiline).push(&["comment"]),
            rule(r"--.*").token(T::CommentSingle),
            rule(r"\s+").token(T::Text),
            rule(r#"""#).token(T::LiteralString).push(&["doublequote"]),
            rule(r"^\s*module\s*").token(T::KeywordNamespace).push(&["imports"]),
            rule(r"^\s*import\s*").token(T::KeywordNamespace).push(&["imports"]),
            rule(r"\[glsl\|.*").token(T::NameEntity).push(&["shader"]),
            rule(r"(import|module|alias|where|port|else|type|case|then|let|as|of|if|in)\b").token(T::KeywordReserved),
            rule(r"[A-Z]\w*").token(T::KeywordType),
            rule(r"^main ").token(T::KeywordReserved),
            rule(r"\((<-|\|\||\|>|&&|\+\+|->|\.\.|//|>>|>=|/=|==|::|<~|<\||<=|<<|~|<|=|:|>|'|/|\\|\.|\^|-|`|\+|\*|\||%)\)").token(T::NameFunction),
            rule(r"(<-|\|\||\|>|&&|\+\+|->|\.\.|//|>>|>=|/=|==|::|<~|<\||<=|<<|~|<|=|:|>|'|/|\\|\.|\^|-|`|\+|\*|\||%)").token(T::NameFunction),
            include("numbers"),
            rule(r"[a-z_][a-zA-Z_\']*").token(T::NameVariable),
            rule(r"[,()\[\]{}]").token(T::Punctuation),
        ]),
        ("comment", &[
            rule(r"-(?!\})").token(T::CommentMultiline),
            rule(r"\{-").token(T::CommentMultiline).push(&["comment"]),
            rule(r"[^-}]").token(T::CommentMultiline),
            rule(r"-\}").token(T::CommentMultiline).pop(1),
        ]),
        ("doublequote", &[
            rule(r"\\u[0-9a-fA-F]{4}").token(T::LiteralStringEscape),
            rule(r#"\\[nrfvb\\"]"#).token(T::LiteralStringEscape),
            rule(r#"[^"]"#).token(T::LiteralString),
            rule(r#"""#).token(T::LiteralString).pop(1),
        ]),
        ("imports", &[
            rule(r"\w+(\.\w+)*").token(T::NameClass).pop(1),
        ]),
        ("numbers", &[
            rule(r"_?\d+\.(?=\d+)").token(T::LiteralNumberFloat),
            rule(r"_?\d+").token(T::LiteralNumberInteger),
        ]),
    ],
};
