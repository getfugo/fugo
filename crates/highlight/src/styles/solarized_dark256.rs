//! Chroma's `solarized-dark256.xml` style, converted to Rust
//! (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "solarized-dark256",
    entries: &[
        (T::Other, "#d75f00"),
        (T::Background, "#8a8a8a bg:#1c1c1c"),
        (T::Keyword, "#5f8700"),
        (T::KeywordConstant, "#d75f00"),
        (T::KeywordDeclaration, "#0087ff"),
        (T::KeywordNamespace, "#d75f00"),
        (T::KeywordReserved, "#0087ff"),
        (T::KeywordType, "#af0000"),
        (T::NameAttribute, "#8a8a8a"),
        (T::NameBuiltin, "#0087ff"),
        (T::NameBuiltinPseudo, "#0087ff"),
        (T::NameClass, "#0087ff"),
        (T::NameConstant, "#d75f00"),
        (T::NameDecorator, "#0087ff"),
        (T::NameEntity, "#d75f00"),
        (T::NameException, "#af8700"),
        (T::NameFunction, "#0087ff"),
        (T::NameTag, "#0087ff"),
        (T::NameVariable, "#0087ff"),
        (T::LiteralString, "#00afaf"),
        (T::LiteralStringBacktick, "#4e4e4e"),
        (T::LiteralStringChar, "#00afaf"),
        (T::LiteralStringDoc, "#00afaf"),
        (T::LiteralStringEscape, "#af0000"),
        (T::LiteralStringHeredoc, "#00afaf"),
        (T::LiteralStringRegex, "#af0000"),
        (T::LiteralNumber, "#00afaf"),
        (T::Operator, "#8a8a8a"),
        (T::OperatorWord, "#5f8700"),
        (T::Comment, "#4e4e4e"),
        (T::CommentSpecial, "#5f8700"),
        (T::CommentPreproc, "#5f8700"),
        (T::GenericDeleted, "#af0000"),
        (T::GenericEmph, "italic"),
        (T::GenericError, "bold #af0000"),
        (T::GenericHeading, "#d75f00"),
        (T::GenericInserted, "#5f8700"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "#0087ff"),
    ],
};
