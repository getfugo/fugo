//! Chroma's `perldoc.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "perldoc",
    entries: &[
        (T::Error, "#a61717 bg:#e3d2d2"),
        (T::Background, "bg:#eeeedd"),
        (T::Keyword, "bold #8b008b"),
        (T::KeywordType, "#00688b"),
        (T::NameAttribute, "#658b00"),
        (T::NameBuiltin, "#658b00"),
        (T::NameClass, "bold #008b45"),
        (T::NameConstant, "#00688b"),
        (T::NameDecorator, "#707a7c"),
        (T::NameException, "bold #008b45"),
        (T::NameFunction, "#008b45"),
        (T::NameNamespace, "underline #008b45"),
        (T::NameTag, "bold #8b008b"),
        (T::NameVariable, "#00688b"),
        (T::LiteralString, "#cd5555"),
        (T::LiteralStringHeredoc, "italic #1c7e71"),
        (T::LiteralStringOther, "#cb6c20"),
        (T::LiteralStringRegex, "#1c7e71"),
        (T::LiteralNumber, "#b452cd"),
        (T::OperatorWord, "#8b008b"),
        (T::Comment, "#228b22"),
        (T::CommentSpecial, "bold #8b008b"),
        (T::CommentPreproc, "#1e889b"),
        (T::GenericDeleted, "#aa0000"),
        (T::GenericEmph, "italic"),
        (T::GenericError, "#aa0000"),
        (T::GenericHeading, "bold #000080"),
        (T::GenericInserted, "#00aa00"),
        (T::GenericOutput, "#888888"),
        (T::GenericPrompt, "#555555"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "bold #800080"),
        (T::GenericTraceback, "#aa0000"),
        (T::GenericUnderline, "underline"),
        (T::TextWhitespace, "#bbbbbb"),
    ],
};
