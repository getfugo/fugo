//! Chroma's `swapoff.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "swapoff",
    entries: &[
        (T::Error, "#ff0000"),
        (T::Background, "#e5e5e5 bg:#000000"),
        (T::Keyword, "bold #ffffff"),
        (T::NameAttribute, "#007f7f"),
        (T::NameBuiltin, "bold #ffffff"),
        (T::NameKeyword, "bold #ffffff"),
        (T::NameTag, "bold"),
        (T::LiteralDate, "bold #ffff00"),
        (T::LiteralString, "bold #00ffff"),
        (T::LiteralNumber, "bold #ffff00"),
        (T::Comment, "#007f7f"),
        (T::CommentPreproc, "bold #00ff00"),
        (T::GenericHeading, "bold"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "bold"),
        (T::GenericUnderline, "underline"),
    ],
};
