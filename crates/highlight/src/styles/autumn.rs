//! Chroma's `autumn.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "autumn",
    entries: &[
        (T::Error, "#ff0000 bg:#ffaaaa"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "#0000aa"),
        (T::KeywordType, "#00aaaa"),
        (T::NameAttribute, "#1e90ff"),
        (T::NameBuiltin, "#00aaaa"),
        (T::NameClass, "underline #00aa00"),
        (T::NameConstant, "#aa0000"),
        (T::NameDecorator, "#888888"),
        (T::NameEntity, "bold #880000"),
        (T::NameFunction, "#00aa00"),
        (T::NameNamespace, "underline #00aaaa"),
        (T::NameTag, "bold #1e90ff"),
        (T::NameVariable, "#aa0000"),
        (T::LiteralString, "#aa5500"),
        (T::LiteralStringRegex, "#009999"),
        (T::LiteralStringSymbol, "#0000aa"),
        (T::LiteralNumber, "#009999"),
        (T::OperatorWord, "#0000aa"),
        (T::Comment, "italic #aaaaaa"),
        (T::CommentSpecial, "italic #0000aa"),
        (T::CommentPreproc, "noitalic #4c8317"),
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
