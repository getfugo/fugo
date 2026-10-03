//! Chroma's `psl.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "psl",
    config: ConfigDef {
        name: "PSL",
        aliases: &["psl"],
        filenames: &["*.psl", "*.BATCH", "*.TRIG", "*.PROC"],
        mime_types: &["text/x-psl"],
        ..ConfigDef::EMPTY
    },
    states: &[
        // NameFunction|TypeName
        ("root", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"\n").token(T::Text),
            rule(r"//.*$").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"\+|-|\*|\/|\b%\b|<|>|=|'|\band\b|\bor\b|_|:|!").token(T::Operator),
            rule(r"[{}(,)\[\]]").token(T::Punctuation).push(&["root"]),
            rule(r"#").token(T::KeywordPseudo).push(&["directive"]),
            rule(r"\.?\d+").token(T::LiteralNumber),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"\b(do|set|if|else|for|while|quit|catch|return|ret|while)\b").token(T::Keyword),
            rule(r"\b(true|false)\b").token(T::KeywordConstant),
            rule(r"\btype\b").token(T::KeywordDeclaration).push(&["typename"]),
            rule(r"\b(public|req|private|void)\b").token(T::KeywordDeclaration),
            rule(r"\b(Boolean|String|Number|Date)\b").token(T::KeywordType),
            rule(r"(\${0,2}[_a-zA-z]\w*)?(\^[_a-zA-Z]\w*)").groups(&[T::NameFunction, T::NameClass]),
            rule(r"([_a-zA-z]\w*)(\.[_a-zA-Z]\w*)(\()").groups(&[T::Name, T::NameFunction, T::Punctuation]),
            rule(r"(\${0,2}[_a-zA-z]\w*)(\.[_a-zA-Z]\w*)").groups(&[T::Name, T::NameProperty]),
            rule(r"\.?(%|\${0,2})[_a-zA-Z]\w*").token(T::Name),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-fA-F0-9]{2,4}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{8}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
        ("typename", &[
            rule(r"\s+").token(T::Text),
            rule(r"\b(public|req|private|void)\b").token(T::KeywordDeclaration),
            rule(r"([_a-zA-Z]\w*)?(\s+)([_a-zA-Z]\w*)").groups(&[T::NameClass, T::Text, T::Name]).pop(1),
            rule("").pop(1),
        ]),
        ("directive", &[
            rule(r"ACCEPT").token(T::KeywordPseudo).push(&["accept-directive"]),
            rule(r"CLASSDEF").token(T::KeywordPseudo).push(&["classdef-directive"]),
            rule(r"IF|ELSEIF").token(T::KeywordPseudo).push(&["if-directive"]),
            rule(r"PACKAGE").token(T::KeywordPseudo).push(&["package-directive"]),
            rule(r"PROPERTYDEF").token(T::KeywordPseudo).pop(1),
            rule(r"INFO|WARN").token(T::KeywordPseudo).push(&["warn-directive"]),
            rule(r"OPTION").token(T::KeywordPseudo).push(&["option-directive"]),
            rule(r"BYPASS|ELSE|END|ENDBYPASS|ENDIF|OPTIMIZE").token(T::KeywordPseudo).push(&["other-directive"]),
        ]),
        ("accept-directive", &[
            rule(r".+$").token(T::CommentSingle),
        ]),
        ("other-directive", &[
            rule(r".+$").token(T::CommentSingle),
        ]),
        ("classdef-directive", &[
            rule(r"\s+").token(T::Text),
            rule(r"delimiter|extends").token(T::Keyword),
            rule(r"public").token(T::KeywordDeclaration),
            rule(r"=").token(T::Operator),
            rule(r"[\w\d]+").token(T::NameClass),
        ]),
        ("if-directive", &[
            rule(r".+$").include("root"),
        ]),
        ("option-directive", &[
            rule(r"\s+").token(T::Text),
            rule(r"ON|OFF").token(T::KeywordConstant).pop(1),
            rule(r"[\w\d]+").token(T::Name),
        ]),
        ("package-directive", &[
            rule(r"\s+").token(T::Text),
            rule(r"\w+").token(T::Name),
            include("root"),
        ]),
    ],
};
