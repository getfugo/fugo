//! Chroma's `solarized-light.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "solarized-light",
    entries: &[
        (T::Background, "bg:#eee8d5"),
        (T::Keyword, "#859900"),
        (T::KeywordConstant, "bold"),
        (T::KeywordNamespace, "bold #dc322f"),
        (T::KeywordType, "bold"),
        (T::Name, "#268bd2"),
        (T::NameBuiltin, "#cb4b16"),
        (T::NameClass, "#cb4b16"),
        (T::NameTag, "bold"),
        (T::Literal, "#2aa198"),
        (T::LiteralNumber, "bold"),
        (T::OperatorWord, "#859900"),
        (T::Comment, "italic #93a1a1"),
        (T::Generic, "#d33682"),
        (T::Text, "#586e75"),
    ],
};
