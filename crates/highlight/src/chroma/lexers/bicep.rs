//! Chroma's `bicep.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "bicep",
    config: ConfigDef {
        name: "Bicep",
        aliases: &["bicep"],
        filenames: &["*.bicep"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("interp", &[
            rule(r"'").token(T::LiteralString).pop(1),
            rule(r"\\.").token(T::LiteralString),
            rule(r"\$\{").token(T::LiteralStringInterpol).push(&["interp-inside"]),
            rule(r"\$").token(T::LiteralString),
            rule(r"[^'\\$]+").token(T::LiteralString),
        ]),
        ("interp-inside", &[
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
            include("root"),
        ]),
        ("commentsandwhitespace", &[
            rule(r"//[^\n\r]+").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"\s+").token(T::TextWhitespace),
        ]),
        ("root", &[
            include("commentsandwhitespace"),
            rule(r"'''.*?'''").token(T::LiteralString),
            rule(r"'").token(T::LiteralString).push(&["interp"]),
            rule(r"#[\w-]+\b").token(T::CommentPreproc),
            rule(r"[\w_]+(?=\()").token(T::NameFunction),
            rule(r"\b(metadata|targetScope|resource|module|param|var|output|for|in|if|existing|import|as|type|with|using|func|assert)\b").token(T::KeywordDeclaration),
            rule(r"\b(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(>=|>|<=|<|==|!=|=~|!~|::|&&|\?\?|!|-|%|\*|\/|\+)").token(T::Operator),
            rule(r"(\(|\)|\[|\]|\.|:|\?|{|}|@|,|\||=>|=)").token(T::Punctuation),
            rule(r"[\w_]+").token(T::NameVariable),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
        ]),
    ],
};
