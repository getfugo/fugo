//! Chroma's `fruity.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "fruity",
    entries: &[
        (T::Background, "#ffffff bg:#111111"),
        (T::Keyword, "bold #fb660a"),
        (T::KeywordPseudo, "nobold"),
        (T::KeywordType, "bold #cdcaa9"),
        (T::NameAttribute, "bold #ff0086"),
        (T::NameConstant, "#0086d2"),
        (T::NameFunction, "bold #ff0086"),
        (T::NameTag, "bold #fb660a"),
        (T::NameVariable, "#fb660a"),
        (T::LiteralString, "#0086d2"),
        (T::LiteralNumber, "bold #0086f7"),
        (T::Comment, "italic #008800 bg:#0f140f"),
        (T::CommentPreproc, "bold #ff0007"),
        (T::GenericHeading, "bold #ffffff"),
        (T::GenericOutput, "#444444 bg:#222222"),
        (T::GenericSubheading, "bold #ffffff"),
        (T::TextWhitespace, "#888888"),
    ],
};
