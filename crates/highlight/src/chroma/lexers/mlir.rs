//! Chroma's `mlir.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "mlir",
    config: ConfigDef {
        name: "MLIR",
        aliases: &["mlir"],
        filenames: &["*.mlir"],
        mime_types: &["text/x-mlir"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("whitespace", &[
            rule(r"(\n|\s)+").token(T::Text),
            rule(r"//.*?\n").token(T::Comment),
        ]),
        ("keyword", &[
            rule(r"(constant|return)").token(T::KeywordType),
            rule(r"(memref|tensor|vector|func|loc)").token(T::KeywordType),
            rule(r"bf16|f16|f32|f64|index").token(T::Keyword),
            rule(r"i[1-9]\d*").token(T::Keyword),
        ]),
        ("root", &[
            include("whitespace"),
            rule(r#"c?"[^"]*?""#).token(T::LiteralString),
            rule(r"\^([-a-zA-Z$._][\w\-$.0-9]*)\s*").token(T::NameLabel),
            rule(r"([\w\d_$.]+)\s*=").token(T::NameLabel),
            include("keyword"),
            rule(r"->").token(T::Punctuation),
            rule(r"@([\w_][\w\d_$.]*)").token(T::NameFunction),
            rule(r"[%#][\w\d_$.]+").token(T::NameVariable),
            rule(r"([1-9?][\d?]*\s*x)+").token(T::LiteralNumber),
            rule(r"0[xX][a-fA-F0-9]+").token(T::LiteralNumber),
            rule(r"-?\d+(?:[.]\d+)?(?:[eE][-+]?\d+(?:[.]\d+)?)?").token(T::LiteralNumber),
            rule(r"[=<>{}\[\]()*.,!:]|x\b").token(T::Punctuation),
            rule(r"[\w\d]+").token(T::Text),
        ]),
    ],
};
