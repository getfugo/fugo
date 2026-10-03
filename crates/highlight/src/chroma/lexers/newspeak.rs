//! Chroma's `newspeak.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "newspeak",
    config: ConfigDef {
        name: "Newspeak",
        aliases: &["newspeak"],
        filenames: &["*.ns2"],
        mime_types: &["text/x-newspeak"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\b(Newsqueak2)\b").token(T::KeywordDeclaration),
            rule(r"'[^']*'").token(T::LiteralString),
            rule(r"\b(class)(\s+)(\w+)(\s*)").groups(&[T::KeywordDeclaration, T::Text, T::NameClass, T::Text]),
            rule(r"\b(mixin|self|super|private|public|protected|nil|true|false)\b").token(T::Keyword),
            rule(r"(\w+\:)(\s*)([a-zA-Z_]\w+)").groups(&[T::NameFunction, T::Text, T::NameVariable]),
            rule(r"(\w+)(\s*)(=)").groups(&[T::NameAttribute, T::Text, T::Operator]),
            rule(r"<\w+>").token(T::CommentSpecial),
            include("expressionstat"),
            include("whitespace"),
        ]),
        ("expressionstat", &[
            rule(r"(\d+\.\d*|\.\d+|\d+[fF])[fF]?").token(T::LiteralNumberFloat),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r":\w+").token(T::NameVariable),
            rule(r"(\w+)(::)").groups(&[T::NameVariable, T::Operator]),
            rule(r"\w+:").token(T::NameFunction),
            rule(r"\w+").token(T::NameVariable),
            rule(r"\(|\)").token(T::Punctuation),
            rule(r"\[|\]").token(T::Punctuation),
            rule(r"\{|\}").token(T::Punctuation),
            rule(r"(\^|\+|\/|~|\*|<|>|=|@|%|\||&|\?|!|,|-|:)").token(T::Operator),
            rule(r"\.|;").token(T::Punctuation),
            include("whitespace"),
            include("literals"),
        ]),
        ("literals", &[
            rule(r"\$.").token(T::LiteralString),
            rule(r"'[^']*'").token(T::LiteralString),
            rule(r"#'[^']*'").token(T::LiteralStringSymbol),
            rule(r"#\w+:?").token(T::LiteralStringSymbol),
            rule(r"#(\+|\/|~|\*|<|>|=|@|%|\||&|\?|!|,|-)+").token(T::LiteralStringSymbol),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::Text),
            rule(r#""[^"]*""#).token(T::Comment),
        ]),
    ],
};
