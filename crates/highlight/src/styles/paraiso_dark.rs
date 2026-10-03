//! Chroma's `paraiso-dark.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "paraiso-dark",
    entries: &[
        (T::Error, "#ef6155"),
        (T::Background, "bg:#2f1e2e"),
        (T::Keyword, "#815ba4"),
        (T::KeywordNamespace, "#5bc4bf"),
        (T::KeywordType, "#fec418"),
        (T::Name, "#e7e9db"),
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
        (T::LiteralStringChar, "#e7e9db"),
        (T::LiteralStringDoc, "#776e71"),
        (T::LiteralStringEscape, "#f99b15"),
        (T::LiteralStringInterpol, "#f99b15"),
        (T::LiteralNumber, "#f99b15"),
        (T::Operator, "#5bc4bf"),
        (T::Punctuation, "#e7e9db"),
        (T::Comment, "#776e71"),
        (T::GenericDeleted, "#ef6155"),
        (T::GenericEmph, "italic"),
        (T::GenericHeading, "bold #e7e9db"),
        (T::GenericInserted, "#48b685"),
        (T::GenericPrompt, "bold #776e71"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "bold #5bc4bf"),
        (T::Text, "#e7e9db"),
    ],
};
