//! Chroma's `borland.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "borland",
    entries: &[
        (T::Error, "#a61717 bg:#e3d2d2"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "bold #000080"),
        (T::NameAttribute, "#ff0000"),
        (T::NameTag, "bold #000080"),
        (T::LiteralString, "#0000ff"),
        (T::LiteralStringChar, "#800080"),
        (T::LiteralNumber, "#0000ff"),
        (T::OperatorWord, "bold"),
        (T::Comment, "italic #008800"),
        (T::CommentSpecial, "bold noitalic"),
        (T::CommentPreproc, "noitalic #008080"),
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
