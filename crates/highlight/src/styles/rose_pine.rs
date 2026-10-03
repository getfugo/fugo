//! Chroma's `rose-pine.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "rose-pine",
    entries: &[
        (T::Error, "#eb6f92"),
        (T::Background, "bg:#191724"),
        (T::Keyword, "#31748f"),
        (T::KeywordNamespace, "#c4a7e7"),
        (T::Name, "#ebbcba"),
        (T::NameAttribute, "#ebbcba"),
        (T::NameClass, "#9ccfd8"),
        (T::NameConstant, "#f6c177"),
        (T::NameDecorator, "#908caa"),
        (T::NameException, "#31748f"),
        (T::NameFunction, "#ebbcba"),
        (T::NameOther, "#e0def4"),
        (T::NameTag, "#ebbcba"),
        (T::Literal, "#f6c177"),
        (T::LiteralDate, "#f6c177"),
        (T::LiteralString, "#f6c177"),
        (T::LiteralStringEscape, "#31748f"),
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
