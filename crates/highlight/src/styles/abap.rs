//! Chroma's `abap.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "abap",
    entries: &[
        (T::Error, "#ff0000"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "#0000ff"),
        (T::Name, "#000000"),
        (T::LiteralString, "#55aa22"),
        (T::LiteralNumber, "#33aaff"),
        (T::OperatorWord, "#0000ff"),
        (T::Comment, "italic #888888"),
        (T::CommentSpecial, "#888888"),
    ],
};
