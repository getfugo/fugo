//! Chroma's `paraiso-light.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "paraiso-light",
    entries: &[
        (T::Error, "#ef6155"),
        (T::Background, "bg:#e7e9db"),
        (T::Keyword, "#815ba4"),
        (T::KeywordNamespace, "#5bc4bf"),
        (T::KeywordType, "#fec418"),
        (T::Name, "#2f1e2e"),
        (T::NameAttribute, "#06b6ef"),
        (T::NameClass, "#fec418"),
        (T::NameConstant, "#ef6155"),
        (T::NameDecorator, "#5bc4bf"),
        (T::NameException, "#ef6155"),
        (T::NameFunction, "#06b6ef"),
        (T::NameNamespace, "#fec418"),
        (T::NameOther, "#06b6ef"),
        (T::NameTag, "#5bc4bf"),
        (T::NameVariable, "#ef6155"),
        (T::Literal, "#f99b15"),
        (T::LiteralDate, "#48b685"),
        (T::LiteralString, "#48b685"),
        (T::LiteralStringChar, "#2f1e2e"),
        (T::LiteralStringDoc, "#8d8687"),
        (T::LiteralStringEscape, "#f99b15"),
        (T::LiteralStringInterpol, "#f99b15"),
        (T::LiteralNumber, "#f99b15"),
        (T::Operator, "#5bc4bf"),
        (T::Punctuation, "#2f1e2e"),
        (T::Comment, "#8d8687"),
        (T::GenericDeleted, "#ef6155"),
        (T::GenericEmph, "italic"),
        (T::GenericHeading, "bold #2f1e2e"),
        (T::GenericInserted, "#48b685"),
        (T::GenericPrompt, "bold #8d8687"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "bold #5bc4bf"),
        (T::Text, "#2f1e2e"),
    ],
};
