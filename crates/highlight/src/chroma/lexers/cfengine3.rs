//! Chroma's `cfengine3.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "cfengine3",
    config: ConfigDef {
        name: "CFEngine3",
        aliases: &["cfengine3", "cf3"],
        filenames: &["*.cf"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("interpol", &[
            rule(r"\$[{(]").token(T::LiteralStringInterpol).push(&[]),
            rule(r"[})]").token(T::LiteralStringInterpol).pop(1),
            rule(r"[^${()}]+").token(T::LiteralStringInterpol),
        ]),
        ("arglist", &[
            rule(r"\)").token(T::Punctuation).pop(1),
            rule(r",").token(T::Punctuation),
            rule(r"\w+").token(T::NameVariable),
            rule(r"\s+").token(T::Text),
        ]),
        ("root", &[
            rule(r"#.*?\n").token(T::Comment),
            rule(r"^@.*?\n").token(T::CommentPreproc),
            rule(r"(body)(\s+)(\S+)(\s+)(control)").groups(&[T::Keyword, T::Text, T::Keyword, T::Text, T::Keyword]),
            rule(r"(body|bundle|promise)(\s+)(\S+)(\s+)(\w+)(\()").groups(&[T::Keyword, T::Text, T::Keyword, T::Text, T::NameFunction, T::Punctuation]).push(&["arglist"]),
            rule(r"(body|bundle|promise)(\s+)(\S+)(\s+)(\w+)").groups(&[T::Keyword, T::Text, T::Keyword, T::Text, T::NameFunction]),
            rule(r"(\S+)(\s*)(=>)(\s*)").groups(&[T::KeywordReserved, T::Text, T::Operator, T::Text]),
            rule(r#"([\w.!&|()"$]+)(::)"#).groups(&[T::NameClass, T::Punctuation]),
            rule(r#"""#).token(T::LiteralString).push(&["doublequotestring"]),
            rule(r"'").token(T::LiteralString).push(&["singlequotestring"]),
            rule(r"`").token(T::LiteralString).push(&["backtickstring"]),
            rule(r"(\w+)(\()").groups(&[T::NameFunction, T::Punctuation]),
            rule(r"(\w+)(:)").groups(&[T::KeywordDeclaration, T::Punctuation]),
            rule(r"@[{(][^)}]+[})]").token(T::NameVariable),
            rule(r"\$[(][^)]+[)]").token(T::NameVariable),
            rule(r"[(){},;]").token(T::Punctuation),
            rule(r"=>").token(T::Operator),
            rule(r"->").token(T::Operator),
            rule(r"\d+\.\d+").token(T::LiteralNumberFloat),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"\w+").token(T::NameFunction),
            rule(r"\s+").token(T::Text),
        ]),
        ("doublequotestring", &[
            rule(r"\$[{(]").token(T::LiteralStringInterpol).push(&["interpol"]),
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r"\n").token(T::LiteralString),
            rule(r".").token(T::LiteralString),
        ]),
        ("singlequotestring", &[
            rule(r"\$[{(]").token(T::LiteralStringInterpol).push(&["interpol"]),
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r"'").token(T::LiteralString).pop(1),
            rule(r"\n").token(T::LiteralString),
            rule(r".").token(T::LiteralString),
        ]),
        ("backtickstring", &[
            rule(r"\$[{(]").token(T::LiteralStringInterpol).push(&["interpol"]),
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r"`").token(T::LiteralString).pop(1),
            rule(r"\n").token(T::LiteralString),
            rule(r".").token(T::LiteralString),
        ]),
    ],
};
