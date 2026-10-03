//! Chroma's `mcfunction.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "mcfunction",
    config: ConfigDef {
        name: "MCFunction",
        aliases: &["mcfunction", "mcf"],
        filenames: &["*.mcfunction"],
        mime_types: &["text/mcfunction"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            include("names"),
            include("comments"),
            include("literals"),
            include("whitespace"),
            include("property"),
            include("operators"),
            include("selectors"),
        ]),
        ("names", &[
            rule(r"^(\s*)([a-z_]+)").groups(&[T::TextWhitespace, T::NameBuiltin]),
            rule(r"(?<=run)\s+[a-z_]+").token(T::NameBuiltin),
            rule(r"\b[0-9a-fA-F]+(?:-[0-9a-fA-F]+){4}\b").token(T::NameVariable),
            include("resource-name"),
            rule(r"[A-Za-z_][\w.#%$]+").token(T::KeywordConstant),
            rule(r"[#%$][\w.#%$]+").token(T::NameVariableMagic),
        ]),
        ("resource-name", &[
            rule(r"#?[a-z_][a-z_.-]*:[a-z0-9_./-]+").token(T::NameFunction),
            rule(r"#?[a-z0-9_\.\-]+\/[a-z0-9_\.\-\/]+").token(T::NameFunction),
        ]),
        ("whitespace", &[
            rule(r"\s+").token(T::TextWhitespace),
        ]),
        ("comments", &[
            rule(r"^\s*(#[>!])").token(T::CommentMultiline).push(&["comments.block", "comments.block.emphasized"]),
            rule(r"#.*$").token(T::CommentSingle),
        ]),
        ("comments.block", &[
            rule(r"^\s*#[>!]").token(T::CommentMultiline).push(&["comments.block.emphasized"]),
            rule(r"^\s*#").token(T::CommentMultiline).push(&["comments.block.normal"]),
            rule("").pop(1),
        ]),
        ("comments.block.normal", &[
            include("comments.block.special"),
            rule(r"\S+").token(T::CommentMultiline),
            rule(r"\n").token(T::Text).pop(1),
            include("whitespace"),
        ]),
        ("comments.block.emphasized", &[
            include("comments.block.special"),
            rule(r"\S+").token(T::LiteralStringDoc),
            rule(r"\n").token(T::Text).pop(1),
            include("whitespace"),
        ]),
        ("comments.block.special", &[
            rule(r"@\S+").token(T::NameDecorator),
            include("resource-name"),
            rule(r"[#%$][\w.#%$]+").token(T::NameVariableMagic),
        ]),
        ("operators", &[
            rule(r"[\-~%^?!+*<>\\/|&=.]").token(T::Operator),
        ]),
        ("literals", &[
            rule(r"\.\.").token(T::Literal),
            rule(r"(true|false)").token(T::KeywordPseudo),
            rule(r"[A-Za-z_]+").token(T::NameVariableClass),
            rule(r"[0-7]b").token(T::LiteralNumberByte),
            rule(r"[+-]?\d*\.?\d+([eE]?[+-]?\d+)?[df]?\b").token(T::LiteralNumberFloat),
            rule(r"[+-]?\d+\b").token(T::LiteralNumberInteger),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["literals.string-double"]),
            rule(r"'").token(T::LiteralStringSingle).push(&["literals.string-single"]),
        ]),
        ("literals.string-double", &[
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
        ("literals.string-single", &[
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r"[^\\'\n]+").token(T::LiteralStringSingle),
            rule(r"'").token(T::LiteralStringSingle).pop(1),
        ]),
        ("selectors", &[
            rule(r"@[a-z]").token(T::NameVariable),
        ]),
        ("property", &[
            rule(r"\{").token(T::Punctuation).push(&["property.curly", "property.key"]),
            rule(r"\[").token(T::Punctuation).push(&["property.square", "property.key"]),
        ]),
        ("property.curly", &[
            include("whitespace"),
            include("property"),
            rule(r"\}").token(T::Punctuation).pop(1),
        ]),
        ("property.square", &[
            include("whitespace"),
            include("property"),
            rule(r"\]").token(T::Punctuation).pop(1),
            rule(r",").token(T::Punctuation),
        ]),
        ("property.key", &[
            include("whitespace"),
            rule(r"#?[a-z_][a-z_\.\-]*\:[a-z0-9_\.\-/]+(?=\s*\=)").token(T::NameAttribute).push(&["property.delimiter"]),
            rule(r"#?[a-z_][a-z0-9_\.\-/]+").token(T::NameAttribute).push(&["property.delimiter"]),
            rule(r"[A-Za-z_\-\+]+").token(T::NameAttribute).push(&["property.delimiter"]),
            rule(r#"""#).token(T::NameAttribute).push(&["property.delimiter"]),
            rule(r"'").token(T::NameAttribute).push(&["property.delimiter"]),
            rule(r"-?\d+").token(T::LiteralNumberInteger).push(&["property.delimiter"]),
            rule("").pop(1),
        ]),
        ("property.key.string-double", &[
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::NameAttribute),
            rule(r#"""#).token(T::NameAttribute).pop(1),
        ]),
        ("property.key.string-single", &[
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r"[^\\'\n]+").token(T::NameAttribute),
            rule(r"'").token(T::NameAttribute).pop(1),
        ]),
        ("property.delimiter", &[
            include("whitespace"),
            rule(r"[:=]!?").token(T::Punctuation).push(&["property.value"]),
            rule(r",").token(T::Punctuation),
            rule("").pop(1),
        ]),
        ("property.value", &[
            include("whitespace"),
            rule(r"#?[a-z_][a-z_\.\-]*\:[a-z0-9_\.\-/]+").token(T::NameTag),
            rule(r"#?[a-z_][a-z0-9_\.\-/]+").token(T::NameTag),
            include("literals"),
            include("property"),
            rule("").pop(1),
        ]),
    ],
};
