//! Chroma's `xcode-dark.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "xcode-dark",
    entries: &[
        (T::Error, "#960050"),
        (T::Background, "#ffffff bg:#1f1f24"),
        (T::Keyword, "#fc5fa3"),
        (T::KeywordConstant, "#fc5fa3"),
        (T::KeywordDeclaration, "#fc5fa3"),
        (T::KeywordReserved, "#fc5fa3"),
        (T::Name, "#ffffff"),
        (T::NameBuiltin, "#d0a8ff"),
        (T::NameBuiltinPseudo, "#a167e6"),
        (T::NameClass, "#5dd8ff"),
        (T::NameFunction, "#41a1c0"),
        (T::NameVariable, "#41a1c0"),
        (T::LiteralString, "#fc6a5d"),
        (T::LiteralStringEscape, "#fc6a5d"),
        (T::LiteralStringInterpol, "#ffffff"),
        (T::LiteralNumber, "#d0bf69"),
        (T::LiteralNumberBin, "#d0bf69"),
        (T::LiteralNumberFloat, "#d0bf69"),
        (T::LiteralNumberHex, "#d0bf69"),
        (T::LiteralNumberInteger, "#d0bf69"),
        (T::LiteralNumberOct, "#d0bf69"),
        (T::Operator, "#ffffff"),
        (T::Punctuation, "#ffffff"),
        (T::Comment, "#6c7986"),
        (T::CommentMultiline, "#6c7986"),
        (T::CommentSingle, "#6c7986"),
        (T::CommentSpecial, "italic #6c7986"),
        (T::CommentPreproc, "#fd8f3f"),
        (T::Text, "#ffffff"),
    ],
};
