//! Chroma's `rose-pine-dawn.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "rose-pine-dawn",
    entries: &[
        (T::Error, "#b4637a"),
        (T::Background, "bg:#faf4ed"),
        (T::Keyword, "#286983"),
        (T::KeywordNamespace, "#907aa9"),
        (T::Name, "#d7827e"),
        (T::NameAttribute, "#d7827e"),
        (T::NameClass, "#56949f"),
        (T::NameConstant, "#ea9d34"),
        (T::NameDecorator, "#797593"),
        (T::NameException, "#286983"),
        (T::NameFunction, "#d7827e"),
        (T::NameOther, "#575279"),
        (T::NameTag, "#d7827e"),
        (T::Literal, "#ea9d34"),
        (T::LiteralDate, "#ea9d34"),
        (T::LiteralString, "#ea9d34"),
        (T::LiteralStringEscape, "#286983"),
        (T::LiteralNumber, "#ea9d34"),
        (T::Operator, "#797593"),
        (T::Punctuation, "#797593"),
        (T::Comment, "#9893a5"),
        (T::GenericDeleted, "#b4637a"),
        (T::GenericEmph, "italic"),
        (T::GenericInserted, "#56949f"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "#907aa9"),
        (T::Text, "#575279"),
    ],
};
