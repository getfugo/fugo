//! Chroma's `algol.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "algol",
    entries: &[
        (T::Error, "border:#ff0000"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "bold underline"),
        (T::KeywordDeclaration, "italic"),
        (T::NameBuiltin, "bold italic"),
        (T::NameBuiltinPseudo, "bold italic"),
        (T::NameClass, "bold italic #666666"),
        (T::NameConstant, "bold italic #666666"),
        (T::NameFunction, "bold italic #666666"),
        (T::NameNamespace, "bold italic #666666"),
        (T::NameVariable, "bold italic #666666"),
        (T::LiteralString, "italic #666666"),
        (T::OperatorWord, "bold"),
        (T::Comment, "italic #888888"),
        (T::CommentSpecial, "bold noitalic #888888"),
        (T::CommentPreproc, "bold noitalic #888888"),
    ],
};
