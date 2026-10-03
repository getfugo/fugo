//! Chroma's `onedark.xml` style, converted to Rust (crate README, "Lexer and style files").

use crate::style::StyleDef;
use crate::token::TokenType as T;

#[rustfmt::skip]
pub(crate) static STYLE: StyleDef = StyleDef {
    name: "onedark",
    entries: &[
        (T::Background, "#ABB2BF bg:#282C34"),
        (T::Punctuation, "#ABB2BF"),
        (T::Keyword, "#C678DD"),
        (T::KeywordConstant, "#E5C07B"),
        (T::KeywordDeclaration, "#C678DD"),
        (T::KeywordNamespace, "#C678DD"),
        (T::KeywordReserved, "#C678DD"),
        (T::KeywordType, "#E5C07B"),
        (T::Name, "#E06C75"),
        (T::NameAttribute, "#E06C75"),
        (T::NameBuiltin, "#E5C07B"),
        (T::NameClass, "#E5C07B"),
        (T::NameFunction, "bold #61AFEF"),
        (T::NameFunctionMagic, "bold #56B6C2"),
        (T::NameOther, "#E06C75"),
        (T::NameTag, "#E06C75"),
        (T::NameDecorator, "#61AFEF"),
        (T::LiteralString, "#98C379"),
        (T::LiteralNumber, "#D19A66"),
        (T::Operator, "#56B6C2"),
        (T::Comment, "#7F848E"),
        (T::GenericDeleted, "#E06C75"),
        (T::GenericInserted, "bold #98C379"),
    ],
};
