//! Chroma's `hr_high_contrast.xml` style, converted to Rust
//! (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "hr_high_contrast",
    entries: &[
        (T::Other, "#d5d500"),
        (T::Background, "#000000"),
        (T::Keyword, "#467faf"),
        (T::Name, "#ffffff"),
        (T::LiteralString, "#a87662"),
        (T::LiteralStringBoolean, "#467faf"),
        (T::LiteralNumber, "#ffffff"),
        (T::Operator, "#e4e400"),
        (T::OperatorWord, "#467faf"),
        (T::Comment, "#5a8349"),
    ],
};
