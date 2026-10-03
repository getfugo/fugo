//! Chroma's `arduino.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "arduino",
    entries: &[
        (T::Error, "#a61717"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "#728e00"),
        (T::KeywordConstant, "#00979d"),
        (T::KeywordPseudo, "#00979d"),
        (T::KeywordReserved, "#00979d"),
        (T::KeywordType, "#00979d"),
        (T::Name, "#434f54"),
        (T::NameBuiltin, "#728e00"),
        (T::NameFunction, "#d35400"),
        (T::NameOther, "#728e00"),
        (T::LiteralString, "#7f8c8d"),
        (T::LiteralNumber, "#8a7b52"),
        (T::Operator, "#728e00"),
        (T::Comment, "#95a5a6"),
        (T::CommentPreproc, "#728e00"),
    ],
};
