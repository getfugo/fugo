//! Chroma's `vim.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "vim",
    entries: &[
        (T::Error, "border:#ff0000"),
        (T::Background, "#cccccc bg:#000000"),
        (T::Keyword, "#cdcd00"),
        (T::KeywordDeclaration, "#00cd00"),
        (T::KeywordNamespace, "#cd00cd"),
        (T::KeywordType, "#00cd00"),
        (T::NameBuiltin, "#cd00cd"),
        (T::NameClass, "#00cdcd"),
        (T::NameException, "bold #666699"),
        (T::NameVariable, "#00cdcd"),
        (T::LiteralString, "#cd0000"),
        (T::LiteralNumber, "#cd00cd"),
        (T::Operator, "#3399cc"),
        (T::OperatorWord, "#cdcd00"),
        (T::Comment, "#000080"),
        (T::CommentSpecial, "bold #cd0000"),
        (T::GenericDeleted, "#cd0000"),
        (T::GenericEmph, "italic"),
        (T::GenericError, "#ff0000"),
        (T::GenericHeading, "bold #000080"),
        (T::GenericInserted, "#00cd00"),
        (T::GenericOutput, "#888888"),
        (T::GenericPrompt, "bold #000080"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "bold #800080"),
        (T::GenericTraceback, "#0044dd"),
        (T::GenericUnderline, "underline"),
    ],
};
