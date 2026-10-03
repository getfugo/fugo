//! Chroma's `bw.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "bw",
    entries: &[
        (T::Error, "border:#ff0000"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "bold"),
        (T::KeywordPseudo, "nobold"),
        (T::KeywordType, "nobold"),
        (T::NameClass, "bold"),
        (T::NameEntity, "bold"),
        (T::NameException, "bold"),
        (T::NameNamespace, "bold"),
        (T::NameTag, "bold"),
        (T::LiteralString, "italic"),
        (T::LiteralStringEscape, "bold"),
        (T::LiteralStringInterpol, "bold"),
        (T::OperatorWord, "bold"),
        (T::Comment, "italic"),
        (T::CommentPreproc, "noitalic"),
        (T::GenericEmph, "italic"),
        (T::GenericHeading, "bold"),
        (T::GenericPrompt, "bold"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "bold"),
    ],
};
