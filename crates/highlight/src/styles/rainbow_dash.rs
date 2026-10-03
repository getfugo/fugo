//! Chroma's `rainbow_dash.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "rainbow_dash",
    entries: &[
        (T::Error, "#ffffff bg:#cc0000"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "bold #2c5dcd"),
        (T::KeywordPseudo, "nobold"),
        (T::KeywordType, "#5918bb"),
        (T::NameAttribute, "italic #2c5dcd"),
        (T::NameBuiltin, "bold #5918bb"),
        (T::NameClass, "underline"),
        (T::NameConstant, "#318495"),
        (T::NameDecorator, "bold #ff8000"),
        (T::NameEntity, "bold #5918bb"),
        (T::NameException, "bold #5918bb"),
        (T::NameFunction, "bold #ff8000"),
        (T::NameTag, "bold #2c5dcd"),
        (T::LiteralString, "#00cc66"),
        (T::LiteralStringDoc, "italic"),
        (T::LiteralStringEscape, "bold #c5060b"),
        (T::LiteralStringOther, "#318495"),
        (T::LiteralStringSymbol, "bold #c5060b"),
        (T::LiteralNumber, "bold #5918bb"),
        (T::Operator, "#2c5dcd"),
        (T::OperatorWord, "bold"),
        (T::Comment, "italic #0080ff"),
        (T::CommentSpecial, "bold"),
        (T::CommentPreproc, "noitalic"),
        (T::GenericDeleted, "bg:#ffcccc border:#c5060b"),
        (T::GenericEmph, "italic"),
        (T::GenericError, "#ff0000"),
        (T::GenericHeading, "bold #2c5dcd"),
        (T::GenericInserted, "bg:#ccffcc border:#00cc00"),
        (T::GenericOutput, "#aaaaaa"),
        (T::GenericPrompt, "bold #2c5dcd"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "bold #2c5dcd"),
        (T::GenericTraceback, "#c5060b"),
        (T::GenericUnderline, "underline"),
        (T::Text, "#4d4d4d"),
        (T::TextWhitespace, "#cbcbcb"),
    ],
};
