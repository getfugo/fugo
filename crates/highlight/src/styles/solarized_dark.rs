//! Chroma's `solarized-dark.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "solarized-dark",
    entries: &[
        (T::Other, "#cb4b16"),
        (T::Background, "#93a1a1 bg:#002b36"),
        (T::Keyword, "#719e07"),
        (T::KeywordConstant, "#cb4b16"),
        (T::KeywordDeclaration, "#268bd2"),
        (T::KeywordReserved, "#268bd2"),
        (T::KeywordType, "#dc322f"),
        (T::NameAttribute, "#93a1a1"),
        (T::NameBuiltin, "#b58900"),
        (T::NameBuiltinPseudo, "#268bd2"),
        (T::NameClass, "#268bd2"),
        (T::NameConstant, "#cb4b16"),
        (T::NameDecorator, "#268bd2"),
        (T::NameEntity, "#cb4b16"),
        (T::NameException, "#cb4b16"),
        (T::NameFunction, "#268bd2"),
        (T::NameTag, "#268bd2"),
        (T::NameVariable, "#268bd2"),
        (T::LiteralString, "#2aa198"),
        (T::LiteralStringBacktick, "#586e75"),
        (T::LiteralStringChar, "#2aa198"),
        (T::LiteralStringDoc, "#93a1a1"),
        (T::LiteralStringEscape, "#cb4b16"),
        (T::LiteralStringHeredoc, "#93a1a1"),
        (T::LiteralStringRegex, "#dc322f"),
        (T::LiteralNumber, "#2aa198"),
        (T::Operator, "#719e07"),
        (T::Comment, "#586e75"),
        (T::CommentSpecial, "#719e07"),
        (T::CommentPreproc, "#719e07"),
        (T::GenericDeleted, "#dc322f"),
        (T::GenericEmph, "italic"),
        (T::GenericError, "bold #dc322f"),
        (T::GenericHeading, "#cb4b16"),
        (T::GenericInserted, "#719e07"),
        (T::GenericStrong, "bold"),
        (T::GenericSubheading, "#268bd2"),
    ],
};
