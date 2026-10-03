//! Chroma's `modus-operandi.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "modus-operandi",
    entries: &[
        (T::Background, "#000000 bg:#ffffff"),
        (T::Keyword, "#5317ac"),
        (T::KeywordConstant, "#0000c0"),
        (T::KeywordType, "#005a5f"),
        (T::NameBuiltin, "#8f0075"),
        (T::NameFunction, "#721045"),
        (T::NameVariable, "#00538b"),
        (T::Literal, "#0000c0"),
        (T::LiteralString, "#2544bb"),
        (T::Operator, "#00538b"),
        (T::Comment, "#505050"),
    ],
};
