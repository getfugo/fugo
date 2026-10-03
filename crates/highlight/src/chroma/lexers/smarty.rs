//! Chroma's `smarty.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "smarty",
    config: ConfigDef {
        name: "Smarty",
        aliases: &["smarty"],
        filenames: &["*.tpl"],
        mime_types: &["application/x-smarty"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"[^{]+").token(T::Other),
            rule(r"(\{)(\*.*?\*)(\})").groups(&[T::CommentPreproc, T::Comment, T::CommentPreproc]),
            rule(r"(\{php\})(.*?)(\{/php\})").bygroups(&[E::Token(T::CommentPreproc), E::Using("PHP"), E::Token(T::CommentPreproc)]),
            rule(r"(\{)(/?[a-zA-Z_]\w*)(\s*)").groups(&[T::CommentPreproc, T::NameFunction, T::Text]).push(&["smarty"]),
            rule(r"\{").token(T::CommentPreproc).push(&["smarty"]),
        ]),
        ("smarty", &[
            rule(r"\s+").token(T::Text),
            rule(r"\{").token(T::CommentPreproc).push(&[]),
            rule(r"\}").token(T::CommentPreproc).pop(1),
            rule(r"#[a-zA-Z_]\w*#").token(T::NameVariable),
            rule(r"\$[a-zA-Z_]\w*(\.\w+)*").token(T::NameVariable),
            rule(r"[~!%^&*()+=|\[\]:;,.<>/?@-]").token(T::Operator),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"[0-9](\.[0-9]*)?(eE[+-][0-9])?[flFLdD]?|0[xX][0-9a-fA-F]+[Ll]?").token(T::LiteralNumber),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"[a-zA-Z_]\w*").token(T::NameAttribute),
        ]),
    ],
};
