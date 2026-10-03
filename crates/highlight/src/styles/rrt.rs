//! Chroma's `rrt.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "rrt",
    entries: &[
        (T::Background, "#f8f8f2 bg:#000000"),
        (T::Keyword, "#ff0000"),
        (T::KeywordType, "#ee82ee"),
        (T::NameConstant, "#7fffd4"),
        (T::NameFunction, "#ffff00"),
        (T::NameVariable, "#eedd82"),
        (T::LiteralString, "#87ceeb"),
        (T::LiteralStringSymbol, "#ff6600"),
        (T::LiteralNumber, "#ff6600"),
        (T::Comment, "#00ff00"),
        (T::CommentPreproc, "#e5e5e5"),
        (T::GenericDeleted, "#f00"),
        (T::GenericEmph, "italic"),
        (T::GenericHeading, "bold #ff0"),
        (T::GenericInserted, "#0f0"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "bold #87ceeb"),
    ],
};
