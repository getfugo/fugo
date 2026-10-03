//! Chroma's `onesenterprise.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "onesenterprise",
    entries: &[
        (T::Keyword, "#ff0000"),
        (T::Name, "#0000ff"),
        (T::LiteralString, "#000000"),
        (T::Operator, "#ff0000"),
        (T::Punctuation, "#ff0000"),
        (T::Comment, "#008000"),
        (T::CommentPreproc, "#963200"),
        (T::Text, "#000000"),
    ],
};
