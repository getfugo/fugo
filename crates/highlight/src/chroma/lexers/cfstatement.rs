//! Chroma's `cfstatement.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "cfstatement",
    config: ConfigDef {
        name: "cfstatement",
        aliases: &["cfs"],
        case_insensitive: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*(?:.|\n)*?\*/").token(T::CommentMultiline),
            rule(r"\+\+|--").token(T::Operator),
            rule(r"[-+*/^&=!]").token(T::Operator),
            rule(r"<=|>=|<|>|==").token(T::Operator),
            rule(r"mod\b").token(T::Operator),
            rule(r"(eq|lt|gt|lte|gte|not|is|and|or)\b").token(T::Operator),
            rule(r"\|\||&&").token(T::Operator),
            rule(r"\?").token(T::Operator),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"'.*?'").token(T::LiteralStringSingle),
            rule(r"\d+").token(T::LiteralNumber),
            rule(r"(if|else|len|var|xml|default|break|switch|component|property|function|do|try|catch|in|continue|for|return|while|required|any|array|binary|boolean|component|date|guid|numeric|query|string|struct|uuid|case)\b").token(T::Keyword),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(application|session|client|cookie|super|this|variables|arguments)\b").token(T::NameConstant),
            rule(r"([a-z_$][\w.]*)(\s*)(\()").groups(&[T::NameFunction, T::Text, T::Punctuation]),
            rule(r"[a-z_$][\w.]*").token(T::NameVariable),
            rule(r"[()\[\]{};:,.\\]").token(T::Punctuation),
            rule(r"\s+").token(T::Text),
        ]),
        ("string", &[
            rule(r#""""#).token(T::LiteralStringDouble),
            rule(r"#.+?#").token(T::LiteralStringInterpol),
            rule(r##"[^"#]+"##).token(T::LiteralStringDouble),
            rule(r"#").token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
    ],
};
