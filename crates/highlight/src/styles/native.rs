//! Chroma's `native.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "native",
    entries: &[
        (T::Error, "#a61717 bg:#e3d2d2"),
        (T::Background, "#d0d0d0 bg:#202020"),
        (T::Keyword, "bold #6ab825"),
        (T::KeywordPseudo, "nobold"),
        (T::NameAttribute, "#bbbbbb"),
        (T::NameBuiltin, "#24909d"),
        (T::NameClass, "underline #447fcf"),
        (T::NameConstant, "#40ffff"),
        (T::NameDecorator, "#ffa500"),
        (T::NameException, "#bbbbbb"),
        (T::NameFunction, "#447fcf"),
        (T::NameNamespace, "underline #447fcf"),
        (T::NameTag, "bold #6ab825"),
        (T::NameVariable, "#40ffff"),
        (T::LiteralString, "#ed9d13"),
        (T::LiteralStringOther, "#ffa500"),
        (T::LiteralNumber, "#3677a9"),
        (T::OperatorWord, "bold #6ab825"),
        (T::Comment, "italic #999999"),
        (T::CommentSpecial, "bold noitalic #e50808 bg:#520000"),
        (T::CommentPreproc, "bold noitalic #cd2828"),
        (T::GenericDeleted, "#d22323"),
        (T::GenericEmph, "italic"),
        (T::GenericError, "#d22323"),
        (T::GenericHeading, "bold #ffffff"),
        (T::GenericInserted, "#589819"),
        (T::GenericOutput, "#cccccc"),
        (T::GenericPrompt, "#aaaaaa"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "underline #ffffff"),
        (T::GenericTraceback, "#d22323"),
        (T::GenericUnderline, "underline"),
        (T::TextWhitespace, "#666666"),
    ],
};
