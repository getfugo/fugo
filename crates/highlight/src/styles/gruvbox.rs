//! Chroma's `gruvbox.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "gruvbox",
    entries: &[
        (T::Background, "noinherit #ebdbb2 bg:#282828"),
        (T::Keyword, "noinherit #fe8019"),
        (T::KeywordType, "noinherit #fabd2f"),
        (T::Name, "#ebdbb2"),
        (T::NameAttribute, "bold #b8bb26"),
        (T::NameBuiltin, "#fabd2f"),
        (T::NameConstant, "noinherit #d3869b"),
        (T::NameEntity, "noinherit #fabd2f"),
        (T::NameException, "noinherit #fb4934"),
        (T::NameFunction, "#fabd2f"),
        (T::NameLabel, "noinherit #fb4934"),
        (T::NameTag, "noinherit #fb4934"),
        (T::NameVariable, "noinherit #ebdbb2"),
        (T::LiteralString, "noinherit #b8bb26"),
        (T::LiteralStringSymbol, "#83a598"),
        (T::LiteralNumber, "noinherit #d3869b"),
        (T::LiteralNumberFloat, "noinherit #d3869b"),
        (T::Operator, "#fe8019"),
        (T::Comment, "italic #928374"),
        (T::CommentPreproc, "noinherit #8ec07c"),
        (T::Generic, "#ebdbb2"),
        (T::GenericDeleted, "noinherit #282828 bg:#fb4934"),
        (T::GenericEmph, "underline #83a598"),
        (T::GenericError, "bold bg:#fb4934"),
        (T::GenericHeading, "bold #b8bb26"),
        (T::GenericInserted, "noinherit #282828 bg:#b8bb26"),
        (T::GenericOutput, "noinherit #504945"),
        (T::GenericPrompt, "#ebdbb2"),
        (T::GenericStrong, "#ebdbb2"),
        (T::GenericSubheading, "bold #b8bb26"),
        (T::GenericTraceback, "bold bg:#fb4934"),
    ],
};
