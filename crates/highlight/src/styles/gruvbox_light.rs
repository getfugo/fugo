//! Chroma's `gruvbox-light.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "gruvbox-light",
    entries: &[
        (T::Background, "noinherit #3c3836 bg:#fbf1c7"),
        (T::Keyword, "noinherit #af3a03"),
        (T::KeywordType, "noinherit #b57614"),
        (T::Name, "#3c3836"),
        (T::NameAttribute, "bold #79740e"),
        (T::NameBuiltin, "#b57614"),
        (T::NameConstant, "noinherit #d3869b"),
        (T::NameEntity, "noinherit #b57614"),
        (T::NameException, "noinherit #fb4934"),
        (T::NameFunction, "#b57614"),
        (T::NameLabel, "noinherit #9d0006"),
        (T::NameTag, "noinherit #9d0006"),
        (T::NameVariable, "noinherit #3c3836"),
        (T::LiteralString, "noinherit #79740e"),
        (T::LiteralStringSymbol, "#076678"),
        (T::LiteralNumber, "noinherit #8f3f71"),
        (T::LiteralNumberFloat, "noinherit #8f3f71"),
        (T::Operator, "#af3a03"),
        (T::Comment, "italic #928374"),
        (T::CommentPreproc, "noinherit #427b58"),
        (T::Generic, "#3c3836"),
        (T::GenericDeleted, "noinherit #282828 bg:#9d0006"),
        (T::GenericEmph, "underline #076678"),
        (T::GenericError, "bold bg:#9d0006"),
        (T::GenericHeading, "bold #79740e"),
        (T::GenericInserted, "noinherit #282828 bg:#79740e"),
        (T::GenericOutput, "noinherit #504945"),
        (T::GenericPrompt, "#3c3836"),
        (T::GenericStrong, "#3c3836"),
        (T::GenericSubheading, "bold #79740e"),
        (T::GenericTraceback, "bold bg:#3c3836"),
    ],
};
