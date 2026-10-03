//! Chroma's `modus-vivendi.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "modus-vivendi",
    entries: &[
        (T::Background, "#ffffff bg:#000000"),
        (T::Keyword, "#b6a0ff"),
        (T::KeywordConstant, "#00bcff"),
        (T::KeywordType, "#6ae4b9"),
        (T::NameBuiltin, "#f78fe7"),
        (T::NameFunction, "#feacd0"),
        (T::NameVariable, "#00d3d0"),
        (T::Literal, "#00bcff"),
        (T::LiteralString, "#79a8ff"),
        (T::Operator, "#00d3d0"),
        (T::Comment, "#a8a8a8"),
    ],
};
