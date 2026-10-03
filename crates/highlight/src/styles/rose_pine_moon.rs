//! Chroma's `rose-pine-moon.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "rose-pine-moon",
    entries: &[
        (T::Error, "#eb6f92"),
        (T::Background, "bg:#232136"),
        (T::Keyword, "#3e8fb0"),
        (T::KeywordNamespace, "#c4a7e7"),
        (T::Name, "#ea9a97"),
        (T::NameAttribute, "#ea9a97"),
        (T::NameClass, "#9ccfd8"),
        (T::NameConstant, "#f6c177"),
        (T::NameDecorator, "#908caa"),
        (T::NameException, "#3e8fb0"),
        (T::NameFunction, "#ea9a97"),
        (T::NameOther, "#e0def4"),
        (T::NameTag, "#ea9a97"),
        (T::Literal, "#f6c177"),
        (T::LiteralDate, "#f6c177"),
        (T::LiteralString, "#f6c177"),
        (T::LiteralStringEscape, "#3e8fb0"),
        (T::LiteralNumber, "#f6c177"),
        (T::Operator, "#908caa"),
        (T::Punctuation, "#908caa"),
        (T::Comment, "#6e6a86"),
        (T::GenericDeleted, "#eb6f92"),
        (T::GenericEmph, "italic"),
        (T::GenericInserted, "#9ccfd8"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "#c4a7e7"),
        (T::Text, "#e0def4"),
    ],
};
