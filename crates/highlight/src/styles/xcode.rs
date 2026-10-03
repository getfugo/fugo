//! Chroma's `xcode.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "xcode",
    entries: &[
        (T::Error, "#000000"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "#a90d91"),
        (T::Name, "#000000"),
        (T::NameAttribute, "#836c28"),
        (T::NameBuiltin, "#a90d91"),
        (T::NameBuiltinPseudo, "#5b269a"),
        (T::NameClass, "#3f6e75"),
        (T::NameDecorator, "#000000"),
        (T::NameFunction, "#000000"),
        (T::NameLabel, "#000000"),
        (T::NameTag, "#000000"),
        (T::NameVariable, "#000000"),
        (T::Literal, "#1c01ce"),
        (T::LiteralString, "#c41a16"),
        (T::LiteralStringChar, "#2300ce"),
        (T::LiteralNumber, "#1c01ce"),
        (T::Operator, "#000000"),
        (T::Comment, "#177500"),
        (T::CommentPreproc, "#633820"),
    ],
};
