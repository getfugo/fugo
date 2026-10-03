//! Chroma's `trac.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "trac",
    entries: &[
        (T::Error, "#a61717 bg:#e3d2d2"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "bold"),
        (T::KeywordType, "#445588"),
        (T::NameAttribute, "#008080"),
        (T::NameBuiltin, "#999999"),
        (T::NameClass, "bold #445588"),
        (T::NameConstant, "#008080"),
        (T::NameEntity, "#800080"),
        (T::NameException, "bold #990000"),
        (T::NameFunction, "bold #990000"),
        (T::NameNamespace, "#555555"),
        (T::NameTag, "#000080"),
        (T::NameVariable, "#008080"),
        (T::LiteralString, "#bb8844"),
        (T::LiteralStringRegex, "#808000"),
        (T::LiteralNumber, "#009999"),
        (T::Operator, "bold"),
        (T::Comment, "italic #999988"),
        (T::CommentSpecial, "bold #999999"),
        (T::CommentPreproc, "bold noitalic #999999"),
        (T::GenericDeleted, "#000000 bg:#ffdddd"),
        (T::GenericEmph, "italic"),
        (T::GenericError, "#aa0000"),
        (T::GenericHeading, "#999999"),
        (T::GenericInserted, "#000000 bg:#ddffdd"),
        (T::GenericOutput, "#888888"),
        (T::GenericPrompt, "#555555"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "#aaaaaa"),
        (T::GenericTraceback, "#aa0000"),
        (T::GenericUnderline, "underline"),
        (T::TextWhitespace, "#bbbbbb"),
    ],
};
