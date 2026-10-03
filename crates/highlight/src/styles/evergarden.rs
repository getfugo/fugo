//! Chroma's `evergarden.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "evergarden",
    entries: &[
        (T::Background, "noinherit #D6CBB4 bg:#252B2E"),
        (T::Keyword, "noinherit #E67E80"),
        (T::KeywordType, "noinherit #DBBC7F"),
        (T::Name, "#D6CBB4"),
        (T::NameAttribute, "bold #D699B6"),
        (T::NameBuiltin, "#D699B6"),
        (T::NameConstant, "noinherit #D699B6"),
        (T::NameEntity, "noinherit #DBBC7F"),
        (T::NameException, "noinherit #E67E80"),
        (T::NameFunction, "#B2C98F"),
        (T::NameLabel, "noinherit #E67E80"),
        (T::NameTag, "noinherit #7a8478"),
        (T::NameVariable, "noinherit #D6CBB4"),
        (T::LiteralString, "noinherit #B2C98F"),
        (T::LiteralStringSymbol, "#E69875"),
        (T::LiteralNumber, "noinherit #D699B6"),
        (T::LiteralNumberFloat, "noinherit #D699B6"),
        (T::Operator, "#7a8478"),
        (T::Comment, "italic #859289"),
        (T::CommentPreproc, "noinherit #E67E80"),
        (T::Generic, "#D6CBB4"),
        (T::GenericDeleted, "noinherit #252B2E bg:#E67E80"),
        (T::GenericEmph, "#6E8585"),
        (T::GenericError, "bold bg:#E67E80"),
        (T::GenericHeading, "bold #D699B6"),
        (T::GenericInserted, "noinherit #252B2E bg:#B2C98F"),
        (T::GenericOutput, "noinherit #6E8585"),
        (T::GenericPrompt, "#D6CBB4"),
        (T::GenericStrong, "#D6CBB4"),
        (T::GenericSubheading, "bold #B2C98F"),
        (T::GenericTraceback, "bold bg:#E67E80"),
    ],
};
