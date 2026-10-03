//! Chroma's `witchhazel.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "witchhazel",
    entries: &[
        (T::Error, "#960050 bg:#1e0010"),
        (T::Background, "bg:#433e56"),
        (T::Keyword, "#c2ffdf"),
        (T::KeywordNamespace, "#ffb8d1"),
        (T::Name, "#f8f8f2"),
        (T::NameAttribute, "#ceb1ff"),
        (T::NameBuiltinPseudo, "#80cbc4"),
        (T::NameClass, "#ceb1ff"),
        (T::NameConstant, "#c5a3ff"),
        (T::NameDecorator, "#ceb1ff"),
        (T::NameException, "#ceb1ff"),
        (T::NameFunction, "#ceb1ff"),
        (T::NameProperty, "#f8f8f2"),
        (T::NameTag, "#ffb8d1"),
        (T::NameVariable, "#f8f8f2"),
        (T::Literal, "#ae81ff"),
        (T::LiteralDate, "#e6db74"),
        (T::LiteralString, "#1bc5e0"),
        (T::LiteralNumber, "#c5a3ff"),
        (T::Operator, "#ffb8d1"),
        (T::Punctuation, "#f8f8f2"),
        (T::Comment, "#b0bec5"),
        (T::GenericDeleted, "#f92672"),
        (T::GenericEmph, "italic"),
        (T::GenericInserted, "#a6e22e"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "#75715e"),
        (T::Text, "#f8f8f2"),
        (T::TextWhitespace, "#a8757b"),
    ],
};
