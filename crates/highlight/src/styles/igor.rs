//! Chroma's `igor.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "igor",
    entries: &[
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "#0000ff"),
        (T::NameClass, "#007575"),
        (T::NameDecorator, "#cc00a3"),
        (T::NameFunction, "#c34e00"),
        (T::LiteralString, "#009c00"),
        (T::Comment, "italic #ff0000"),
    ],
};
