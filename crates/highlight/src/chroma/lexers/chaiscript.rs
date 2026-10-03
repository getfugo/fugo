//! Chroma's `chaiscript.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "chaiscript",
    config: ConfigDef {
        name: "ChaiScript",
        aliases: &["chai", "chaiscript"],
        filenames: &["*.chai"],
        mime_types: &["text/x-chaiscript", "application/x-chaiscript"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("dqstring", &[
            rule(r#"\$\{[^"}]+?\}"#).token(T::LiteralStringInterpol),
            rule(r"\$").token(T::LiteralStringDouble),
            rule(r"\\\\").token(T::LiteralStringDouble),
            rule(r#"\\""#).token(T::LiteralStringDouble),
            rule(r#"[^\\"$]+"#).token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
        ("commentsandwhitespace", &[
            rule(r"\s+").token(T::Text),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"^\#.*?\n").token(T::CommentSingle),
        ]),
        ("slashstartsregex", &[
            include("commentsandwhitespace"),
            rule(r"/(\\.|[^[/\\\n]|\[(\\.|[^\]\\\n])*])+/([gim]+\b|\B)").token(T::LiteralStringRegex).pop(1),
            rule(r"(?=/)").token(T::Text).push(&["#pop", "badregex"]),
            rule("").pop(1),
        ]),
        ("badregex", &[
            rule(r"\n").token(T::Text).pop(1),
        ]),
        ("root", &[
            include("commentsandwhitespace"),
            rule(r"\n").token(T::Text),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\+\+|--|~|&&|\?|:|\|\||\\(?=\n)|\.\.(<<|>>>?|==?|!=?|[-<>+*%&|^/])=?").token(T::Operator).push(&["slashstartsregex"]),
            rule(r"[{(\[;,]").token(T::Punctuation).push(&["slashstartsregex"]),
            rule(r"[})\].]").token(T::Punctuation),
            rule(r"[=+\-*/]").token(T::Operator),
            rule(r"(for|in|while|do|break|return|continue|if|else|throw|try|catch)\b").token(T::Keyword).push(&["slashstartsregex"]),
            rule(r"(var)\b").token(T::KeywordDeclaration).push(&["slashstartsregex"]),
            rule(r"(attr|def|fun)\b").token(T::KeywordReserved),
            rule(r"(true|false)\b").token(T::KeywordConstant),
            rule(r"(eval|throw)\b").token(T::NameBuiltin),
            rule(r"`\S+`").token(T::NameBuiltin),
            rule(r"[$a-zA-Z_]\w*").token(T::NameOther),
            rule(r"[0-9][0-9]*\.[0-9]+([eE][0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["dqstring"]),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
        ]),
    ],
};
