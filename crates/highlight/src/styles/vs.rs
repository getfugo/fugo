//! Chroma's `vs.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "vs",
    entries: &[
        (T::Error, "border:#ff0000"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "#0000ff"),
        (T::KeywordType, "#2b91af"),
        (T::NameClass, "#2b91af"),
        (T::LiteralString, "#a31515"),
        (T::OperatorWord, "#0000ff"),
        (T::Comment, "#008000"),
        (T::CommentPreproc, "#0000ff"),
        (T::GenericEmph, "italic"),
        (T::GenericHeading, "bold"),
        (T::GenericPrompt, "bold"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "bold"),
    ],
};
