//! Chroma's `github.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "github",
    entries: &[
        (T::Error, "#f6f8fa bg:#82071e"),
        (T::Background, "bg:#ffffff"),
        (T::Keyword, "#cf222e"),
        (T::KeywordType, "#cf222e"),
        (T::NameAttribute, "#1f2328"),
        (T::NameBuiltin, "#6639ba"),
        (T::NameBuiltinPseudo, "#6a737d"),
        (T::NameClass, "#1f2328"),
        (T::NameConstant, "#0550ae"),
        (T::NameDecorator, "#0550ae"),
        (T::NameEntity, "#6639ba"),
        (T::NameFunction, "#6639ba"),
        (T::NameLabel, "bold #990000"),
        (T::NameNamespace, "#24292e"),
        (T::NameOther, "#1f2328"),
        (T::NameTag, "#0550ae"),
        (T::NameVariable, "#953800"),
        (T::NameVariableClass, "#953800"),
        (T::NameVariableGlobal, "#953800"),
        (T::NameVariableInstance, "#953800"),
        (T::LiteralString, "#0a3069"),
        (T::LiteralStringRegex, "#0a3069"),
        (T::LiteralStringSymbol, "#032f62"),
        (T::LiteralNumber, "#0550ae"),
        (T::Operator, "#0550ae"),
        (T::Comment, "#57606a"),
        (T::CommentMultiline, "#57606a"),
        (T::CommentSingle, "#57606a"),
        (T::CommentSpecial, "#57606a"),
        (T::CommentPreproc, "#57606a"),
        (T::GenericDeleted, "#82071e bg:#ffebe9"),
        (T::GenericEmph, "#1f2328"),
        (T::GenericInserted, "#116329 bg:#dafbe1"),
        (T::GenericOutput, "#1f2328"),
        (T::GenericUnderline, "underline"),
        (T::Punctuation, "#1f2328"),
        (T::TextWhitespace, "#ffffff"),
    ],
};
