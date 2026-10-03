//! Chroma's `monokai.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "monokai",
    entries: &[
        (T::Error, "#960050 bg:#1e0010"),
        (T::Background, "bg:#272822"),
        (T::Keyword, "#66d9ef"),
        (T::KeywordNamespace, "#f92672"),
        (T::Name, "#f8f8f2"),
        (T::NameAttribute, "#a6e22e"),
        (T::NameClass, "#a6e22e"),
        (T::NameConstant, "#66d9ef"),
        (T::NameDecorator, "#a6e22e"),
        (T::NameException, "#a6e22e"),
        (T::NameFunction, "#a6e22e"),
        (T::NameOther, "#a6e22e"),
        (T::NameTag, "#f92672"),
        (T::Literal, "#ae81ff"),
        (T::LiteralDate, "#e6db74"),
        (T::LiteralString, "#e6db74"),
        (T::LiteralStringEscape, "#ae81ff"),
        (T::LiteralNumber, "#ae81ff"),
        (T::Operator, "#f92672"),
        (T::Punctuation, "#f8f8f2"),
        (T::Comment, "#75715e"),
        (T::GenericDeleted, "#f92672"),
        (T::GenericEmph, "italic"),
        (T::GenericInserted, "#a6e22e"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "#75715e"),
        (T::Text, "#f8f8f2"),
    ],
};
