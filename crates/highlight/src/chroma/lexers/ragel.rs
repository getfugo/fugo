//! Chroma's `ragel.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "ragel",
    config: ConfigDef {
        name: "Ragel",
        aliases: &["ragel"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("host", &[
            rule(r#"([^{}\'"/#]+|[^\\]\\[{}]|"(\\\\|\\"|[^"])*"|'(\\\\|\\'|[^'])*'|//.*$\n?|/\*(.|\n)*?\*/|\#.*$\n?|/(?!\*)(\\\\|\\/|[^/])*/|/)+"#).token(T::Other),
            rule(r"\{").token(T::Punctuation).push(&[]),
            rule(r"\}").token(T::Punctuation).pop(1),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::TextWhitespace),
        ]),
        ("numbers", &[
            rule(r"0x[0-9A-Fa-f]+").token(T::LiteralNumberHex),
            rule(r"[+-]?[0-9]+").token(T::LiteralNumberInteger),
        ]),
        ("literals", &[
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralString),
            rule(r"\[(\\\\|\\\]|[^\]])*\]").token(T::LiteralString),
            rule(r"/(?!\*)(\\\\|\\/|[^/])*/").token(T::LiteralStringRegex),
        ]),
        ("keywords", &[
            rule(r"(access|action|alphtype)\b").token(T::Keyword),
            rule(r"(getkey|write|machine|include)\b").token(T::Keyword),
            rule(r"(any|ascii|extend|alpha|digit|alnum|lower|upper)\b").token(T::Keyword),
            rule(r"(xdigit|cntrl|graph|print|punct|space|zlen|empty)\b").token(T::Keyword),
        ]),
        ("identifiers", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameVariable),
        ]),
        ("root", &[
            include("literals"),
            include("whitespace"),
            include("comments"),
            include("keywords"),
            include("numbers"),
            include("identifiers"),
            include("operators"),
            rule(r"\{").token(T::Punctuation).push(&["host"]),
            rule(r"=").token(T::Operator),
            rule(r";").token(T::Punctuation),
        ]),
        ("comments", &[
            rule(r"\#.*$").token(T::Comment),
        ]),
        ("operators", &[
            rule(r",").token(T::Operator),
            rule(r"\||&|--?").token(T::Operator),
            rule(r"\.|<:|:>>?").token(T::Operator),
            rule(r":").token(T::Operator),
            rule(r"->").token(T::Operator),
            rule(r"(>|\$|%|<|@|<>)(/|eof\b)").token(T::Operator),
            rule(r"(>|\$|%|<|@|<>)(!|err\b)").token(T::Operator),
            rule(r"(>|\$|%|<|@|<>)(\^|lerr\b)").token(T::Operator),
            rule(r"(>|\$|%|<|@|<>)(~|to\b)").token(T::Operator),
            rule(r"(>|\$|%|<|@|<>)(\*|from\b)").token(T::Operator),
            rule(r">|@|\$|%").token(T::Operator),
            rule(r"\*|\?|\+|\{[0-9]*,[0-9]*\}").token(T::Operator),
            rule(r"!|\^").token(T::Operator),
            rule(r"\(|\)").token(T::Operator),
        ]),
    ],
};
