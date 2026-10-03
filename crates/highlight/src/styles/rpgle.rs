//! Chroma's `rpgle.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "RPGLE",
    entries: &[
        (T::Error, "#960050 bg:#1e0010"),
        (T::Background, "bg:#fafafa"),
        (T::Keyword, "#00a8c8"),
        (T::KeywordNamespace, "#f92672"),
        (T::KeywordReserved, "#0000ff"),
        (T::KeywordType, "#800000"),
        (T::Name, "#111111"),
        (T::NameAttribute, "#75af00"),
        (T::NameClass, "#75af00"),
        (T::NameConstant, "#00a8c8"),
        (T::NameDecorator, "#75af00"),
        (T::NameException, "#75af00"),
        (T::NameFunction, "#75af00"),
        (T::NameOther, "#75af00"),
        (T::NameTag, "#f92672"),
        (T::Literal, "#ae81ff"),
        (T::LiteralDate, "#d88200"),
        (T::LiteralString, "#d88200"),
        (T::LiteralStringEscape, "#8045ff"),
        (T::LiteralNumber, "#ae81ff"),
        (T::Operator, "#f92672"),
        (T::Punctuation, "#ff0000"),
        (T::Comment, "#75715e"),
        (T::CommentPreproc, "#2e7d32"),
        (T::CommentSpecial, "#ffbf00"),
        (T::GenericEmph, "italic"),
        (T::GenericStrong, "bold"),
        (T::Text, "#272822"),
    ],
};
