//! Chroma's `hrdark.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "hrdark",
    entries: &[
        (T::Other, "#ffffff"),
        (T::Background, "#1d2432"),
        (T::Keyword, "#ff636f"),
        (T::Name, "#58a1dd"),
        (T::Literal, "#a6be9d"),
        (T::Operator, "#ff636f"),
        (T::OperatorWord, "#ff636f"),
        (T::Comment, "italic #828b96"),
    ],
};
