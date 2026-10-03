//! Chroma's `smalltalk.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "smalltalk",
    config: ConfigDef {
        name: "Smalltalk",
        aliases: &["smalltalk", "squeak", "st"],
        filenames: &["*.st"],
        mime_types: &["text/x-smalltalk"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("inner_parenth", &[
            rule(r"\)").token(T::LiteralStringSymbol).pop(1),
            include("_parenth_helper"),
        ]),
        ("objects", &[
            rule(r"\[").token(T::Text).push(&["blockvariables"]),
            rule(r"\]").token(T::Text).push(&["afterobject"]),
            rule(r"\b(self|super|true|false|nil|thisContext)\b").token(T::NameBuiltinPseudo).push(&["afterobject"]),
            rule(r"\b[A-Z]\w*(?!:)\b").token(T::NameClass).push(&["afterobject"]),
            rule(r"\b[a-z]\w*(?!:)\b").token(T::NameVariable).push(&["afterobject"]),
            rule(r#"#("(""|[^"])*"|[-+*/\\~<>=|&!?,@%]+|[\w:]+)"#).token(T::LiteralStringSymbol).push(&["afterobject"]),
            include("literals"),
        ]),
        ("afterobject", &[
            rule(r"! !$").token(T::Keyword).pop(1),
            include("whitespaces"),
            rule(r"\b(ifTrue:|ifFalse:|whileTrue:|whileFalse:|timesRepeat:)").token(T::NameBuiltin).pop(1),
            rule(r"\b(new\b(?!:))").token(T::NameBuiltin),
            rule(r":=|_").token(T::Operator).pop(1),
            rule(r"\b[a-zA-Z]+\w*:").token(T::NameFunction).pop(1),
            rule(r"\b[a-zA-Z]+\w*").token(T::NameFunction),
            rule(r"\w+:?|[-+*/\\~<>=|&!?,@%]+").token(T::NameFunction).pop(1),
            rule(r"\.").token(T::Punctuation).pop(1),
            rule(r";").token(T::Punctuation),
            rule(r"[\])}]").token(T::Text),
            rule(r"[\[({]").token(T::Text).pop(1),
        ]),
        ("literals", &[
            rule(r"'(''|[^'])*'").token(T::LiteralString).push(&["afterobject"]),
            rule(r"\$.").token(T::LiteralStringChar).push(&["afterobject"]),
            rule(r"#\(").token(T::LiteralStringSymbol).push(&["parenth"]),
            rule(r"\)").token(T::Text).push(&["afterobject"]),
            rule(r"(\d+r)?-?\d+(\.\d+)?(e-?\d+)?").token(T::LiteralNumber).push(&["afterobject"]),
        ]),
        ("root", &[
            rule(r"(<)(\w+:)(.*?)(>)").groups(&[T::Text, T::Keyword, T::Text, T::Text]),
            include("squeak fileout"),
            include("whitespaces"),
            include("method definition"),
            rule(r"(\|)([\w\s]*)(\|)").groups(&[T::Operator, T::NameVariable, T::Operator]),
            include("objects"),
            rule(r"\^|:=|_").token(T::Operator),
            rule(r"[\]({}.;!]").token(T::Text),
        ]),
        ("_parenth_helper", &[
            include("whitespaces"),
            rule(r"(\d+r)?-?\d+(\.\d+)?(e-?\d+)?").token(T::LiteralNumber),
            rule(r"[-+*/\\~<>=|&#!?,@%\w:]+").token(T::LiteralStringSymbol),
            rule(r"'(''|[^'])*'").token(T::LiteralString),
            rule(r"\$.").token(T::LiteralStringChar),
            rule(r"#*\(").token(T::LiteralStringSymbol).push(&["inner_parenth"]),
        ]),
        ("parenth", &[
            rule(r"\)").token(T::LiteralStringSymbol).push(&["root", "afterobject"]),
            include("_parenth_helper"),
        ]),
        ("whitespaces", &[
            rule(r"\s+").token(T::Text),
            rule(r#""(""|[^"])*""#).token(T::Comment),
        ]),
        ("squeak fileout", &[
            rule(r#"^"(""|[^"])*"!"#).token(T::Keyword),
            rule(r"^'(''|[^'])*'!").token(T::Keyword),
            rule(r"^(!)(\w+)( commentStamp: )(.*?)( prior: .*?!\n)(.*?)(!)").groups(&[T::Keyword, T::NameClass, T::Keyword, T::LiteralString, T::Keyword, T::Text, T::Keyword]),
            rule(r"^(!)(\w+(?: class)?)( methodsFor: )('(?:''|[^'])*')(.*?!)").groups(&[T::Keyword, T::NameClass, T::Keyword, T::LiteralString, T::Keyword]),
            rule(r"^(\w+)( subclass: )(#\w+)(\s+instanceVariableNames: )(.*?)(\s+classVariableNames: )(.*?)(\s+poolDictionaries: )(.*?)(\s+category: )(.*?)(!)").groups(&[T::NameClass, T::Keyword, T::LiteralStringSymbol, T::Keyword, T::LiteralString, T::Keyword, T::LiteralString, T::Keyword, T::LiteralString, T::Keyword, T::LiteralString, T::Keyword]),
            rule(r"^(\w+(?: class)?)(\s+instanceVariableNames: )(.*?)(!)").groups(&[T::NameClass, T::Keyword, T::LiteralString, T::Keyword]),
            rule(r"(!\n)(\].*)(! !)$").groups(&[T::Keyword, T::Text, T::Keyword]),
            rule(r"! !$").token(T::Keyword),
        ]),
        ("method definition", &[
            rule(r"([a-zA-Z]+\w*:)(\s*)(\w+)").groups(&[T::NameFunction, T::Text, T::NameVariable]),
            rule(r"^(\b[a-zA-Z]+\w*\b)(\s*)$").groups(&[T::NameFunction, T::Text]),
            rule(r"^([-+*/\\~<>=|&!?,@%]+)(\s*)(\w+)(\s*)$").groups(&[T::NameFunction, T::Text, T::NameVariable, T::Text]),
        ]),
        ("blockvariables", &[
            include("whitespaces"),
            rule(r"(:)(\s*)(\w+)").groups(&[T::Operator, T::Text, T::NameVariable]),
            rule(r"\|").token(T::Operator).pop(1),
            rule("").pop(1),
        ]),
    ],
};
