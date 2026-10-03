//! Chroma's `antlr.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "antlr",
    config: ConfigDef {
        name: "ANTLR",
        aliases: &["antlr"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("nested-arg-action", &[
            rule(r#"([^$\[\]\'"/]+|"(\\\\|\\"|[^"])*"|'(\\\\|\\'|[^'])*'|//.*$\n?|/\*(.|\n)*?\*/|/(?!\*)(\\\\|\\/|[^/])*/|/)+"#).token(T::Other),
            rule(r"\[").token(T::Punctuation).push(&[]),
            rule(r"\]").token(T::Punctuation).pop(1),
            rule(r"(\$[a-zA-Z]+)(\.?)(text|value)?").groups(&[T::NameVariable, T::Punctuation, T::NameProperty]),
            rule(r"(\\\\|\\\]|\\\[|[^\[\]])+").token(T::Other),
        ]),
        ("exception", &[
            rule(r"\n").token(T::TextWhitespace).pop(1),
            rule(r"\s").token(T::TextWhitespace),
            include("comments"),
            rule(r"\[").token(T::Punctuation).push(&["nested-arg-action"]),
            rule(r"\{").token(T::Punctuation).push(&["action"]),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::TextWhitespace),
        ]),
        ("root", &[
            include("whitespace"),
            include("comments"),
            rule(r"(lexer|parser|tree)?(\s*)(grammar\b)(\s*)([A-Za-z]\w*)(;)").groups(&[T::Keyword, T::TextWhitespace, T::Keyword, T::TextWhitespace, T::NameClass, T::Punctuation]),
            rule(r"options\b").token(T::Keyword).push(&["options"]),
            rule(r"tokens\b").token(T::Keyword).push(&["tokens"]),
            rule(r"(scope)(\s*)([A-Za-z]\w*)(\s*)(\{)").groups(&[T::Keyword, T::TextWhitespace, T::NameVariable, T::TextWhitespace, T::Punctuation]).push(&["action"]),
            rule(r"(catch|finally)\b").token(T::Keyword).push(&["exception"]),
            rule(r"(@[A-Za-z]\w*)(\s*)(::)?(\s*)([A-Za-z]\w*)(\s*)(\{)").groups(&[T::NameLabel, T::TextWhitespace, T::Punctuation, T::TextWhitespace, T::NameLabel, T::TextWhitespace, T::Punctuation]).push(&["action"]),
            rule(r"((?:protected|private|public|fragment)\b)?(\s*)([A-Za-z]\w*)(!)?").groups(&[T::Keyword, T::TextWhitespace, T::NameLabel, T::Punctuation]).push(&["rule-alts", "rule-prelims"]),
        ]),
        ("tokens", &[
            include("whitespace"),
            include("comments"),
            rule(r"\{").token(T::Punctuation),
            rule(r"([A-Z]\w*)(\s*)(=)?(\s*)(\'(?:\\\\|\\\'|[^\']*)\')?(\s*)(;)").groups(&[T::NameLabel, T::TextWhitespace, T::Punctuation, T::TextWhitespace, T::LiteralString, T::TextWhitespace, T::Punctuation]),
            rule(r"\}").token(T::Punctuation).pop(1),
        ]),
        ("options", &[
            include("whitespace"),
            include("comments"),
            rule(r"\{").token(T::Punctuation),
            rule(r"([A-Za-z]\w*)(\s*)(=)(\s*)([A-Za-z]\w*|\'(?:\\\\|\\\'|[^\']*)\'|[0-9]+|\*)(\s*)(;)").groups(&[T::NameVariable, T::TextWhitespace, T::Punctuation, T::TextWhitespace, T::Text, T::TextWhitespace, T::Punctuation]),
            rule(r"\}").token(T::Punctuation).pop(1),
        ]),
        ("rule-alts", &[
            include("whitespace"),
            include("comments"),
            rule(r"options\b").token(T::Keyword).push(&["options"]),
            rule(r":").token(T::Punctuation),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralString),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"<<([^>]|>[^>])>>").token(T::LiteralString),
            rule(r"\$?[A-Z_]\w*").token(T::NameConstant),
            rule(r"\$?[a-z_]\w*").token(T::NameVariable),
            rule(r"(\+|\||->|=>|=|\(|\)|\.\.|\.|\?|\*|\^|!|\#|~)").token(T::Operator),
            rule(r",").token(T::Punctuation),
            rule(r"\[").token(T::Punctuation).push(&["nested-arg-action"]),
            rule(r"\{").token(T::Punctuation).push(&["action"]),
            rule(r";").token(T::Punctuation).pop(1),
        ]),
        ("rule-prelims", &[
            include("whitespace"),
            include("comments"),
            rule(r"returns\b").token(T::Keyword),
            rule(r"\[").token(T::Punctuation).push(&["nested-arg-action"]),
            rule(r"\{").token(T::Punctuation).push(&["action"]),
            rule(r"(throws)(\s+)([A-Za-z]\w*)").groups(&[T::Keyword, T::TextWhitespace, T::NameLabel]),
            rule(r"(,)(\s*)([A-Za-z]\w*)").groups(&[T::Punctuation, T::TextWhitespace, T::NameLabel]),
            rule(r"options\b").token(T::Keyword).push(&["options"]),
            rule(r"(scope)(\s+)(\{)").groups(&[T::Keyword, T::TextWhitespace, T::Punctuation]).push(&["action"]),
            rule(r"(scope)(\s+)([A-Za-z]\w*)(\s*)(;)").groups(&[T::Keyword, T::TextWhitespace, T::NameLabel, T::TextWhitespace, T::Punctuation]),
            rule(r"(@[A-Za-z]\w*)(\s*)(\{)").groups(&[T::NameLabel, T::TextWhitespace, T::Punctuation]).push(&["action"]),
            rule(r":").token(T::Punctuation).pop(1),
        ]),
        ("action", &[
            rule(r#"([^${}\'"/\\]+|"(\\\\|\\"|[^"])*"|'(\\\\|\\'|[^'])*'|//.*$\n?|/\*(.|\n)*?\*/|/(?!\*)(\\\\|\\/|[^/])*/|\\(?!%)|/)+"#).token(T::Other),
            rule(r"(\\)(%)").groups(&[T::Punctuation, T::Other]),
            rule(r"(\$[a-zA-Z]+)(\.?)(text|value)?").groups(&[T::NameVariable, T::Punctuation, T::NameProperty]),
            rule(r"\{").token(T::Punctuation).push(&[]),
            rule(r"\}").token(T::Punctuation).pop(1),
        ]),
        ("comments", &[
            rule(r"//.*$").token(T::Comment),
            rule(r"/\*(.|\n)*?\*/").token(T::Comment),
        ]),
    ],
};
