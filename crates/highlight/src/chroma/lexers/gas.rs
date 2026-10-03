//! Chroma's `gas.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "gas",
    config: ConfigDef {
        name: "GAS",
        aliases: &["gas", "asm"],
        filenames: &["*.s", "*.S"],
        mime_types: &["text/x-gas"],
        priority: 0.1,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("punctuation", &[
            rule(r"[-*,.()\[\]!:]+").token(T::Punctuation),
        ]),
        ("root", &[
            include("whitespace"),
            rule(r"(?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+):").token(T::NameLabel),
            rule(r"\.(?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+)").token(T::NameAttribute).push(&["directive-args"]),
            rule(r"lock|rep(n?z)?|data\d+").token(T::NameAttribute),
            rule(r"(?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+)").token(T::NameFunction).push(&["instruction-args"]),
            rule(r"[\r\n]+").token(T::Text),
        ]),
        ("directive-args", &[
            rule(r"(?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+)").token(T::NameConstant),
            rule(r#""(\\"|[^"])*""#).token(T::LiteralString),
            rule(r"@(?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+)").token(T::NameAttribute),
            rule(r"(?:0[xX][a-zA-Z0-9]+|\d+)").token(T::LiteralNumberInteger),
            rule(r"%(?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+)").token(T::NameVariable),
            rule(r"[\r\n]+").token(T::Text).pop(1),
            rule(r"([;#]|//).*?\n").token(T::CommentSingle).pop(1),
            rule(r"/[*].*?[*]/").token(T::CommentMultiline),
            rule(r"/[*].*?\n[\w\W]*?[*]/").token(T::CommentMultiline).pop(1),
            include("punctuation"),
            include("whitespace"),
        ]),
        ("instruction-args", &[
            rule(r"([a-z0-9]+)( )(<)((?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+))(>)").groups(&[T::LiteralNumberHex, T::Text, T::Punctuation, T::NameConstant, T::Punctuation]),
            rule(r"([a-z0-9]+)( )(<)((?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+))([-+])((?:0[xX][a-zA-Z0-9]+|\d+))(>)").groups(&[T::LiteralNumberHex, T::Text, T::Punctuation, T::NameConstant, T::Punctuation, T::LiteralNumberInteger, T::Punctuation]),
            rule(r"(?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+)").token(T::NameConstant),
            rule(r"(?:0[xX][a-zA-Z0-9]+|\d+)").token(T::LiteralNumberInteger),
            rule(r"%(?:[a-zA-Z$_][\w$.@-]*|\.[\w$.@-]+)").token(T::NameVariable),
            rule(r"$(?:0[xX][a-zA-Z0-9]+|\d+)").token(T::LiteralNumberInteger),
            rule(r"$'(.|\\')'").token(T::LiteralStringChar),
            rule(r"[\r\n]+").token(T::Text).pop(1),
            rule(r"([;#]|//).*?\n").token(T::CommentSingle).pop(1),
            rule(r"/[*].*?[*]/").token(T::CommentMultiline),
            rule(r"/[*].*?\n[\w\W]*?[*]/").token(T::CommentMultiline).pop(1),
            include("punctuation"),
            include("whitespace"),
        ]),
        ("whitespace", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"([;#]|//).*?\n").token(T::CommentSingle),
            rule(r"/[*][\w\W]*?[*]/").token(T::CommentMultiline),
        ]),
    ],
};
