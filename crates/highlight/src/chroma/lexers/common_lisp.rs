//! Chroma's `common_lisp.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "common_lisp",
    config: ConfigDef {
        name: "Common Lisp",
        aliases: &["common-lisp", "cl", "lisp"],
        filenames: &["*.cl", "*.lisp"],
        mime_types: &["text/x-common-lisp"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("body", &[
            rule(r"\s+").token(T::Text),
            rule(r";.*$").token(T::CommentSingle),
            rule(r"#\|").token(T::CommentMultiline).push(&["multiline-comment"]),
            rule(r"#\d*Y.*$").token(T::CommentSpecial),
            rule(r#""(\\.|\\\n|[^"\\])*""#).token(T::LiteralString),
            rule(r":(\|[^|]+\||(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~])(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~]|[#.:])*)").token(T::LiteralStringSymbol),
            rule(r"::(\|[^|]+\||(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~])(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~]|[#.:])*)").token(T::LiteralStringSymbol),
            rule(r":#(\|[^|]+\||(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~])(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~]|[#.:])*)").token(T::LiteralStringSymbol),
            rule(r"'(\|[^|]+\||(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~])(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~]|[#.:])*)").token(T::LiteralStringSymbol),
            rule(r"'").token(T::Operator),
            rule(r"`").token(T::Operator),
            rule(r#"[-+]?\d+\.?(?=[ "()\'\n,;`])"#).token(T::LiteralNumberInteger),
            rule(r#"[-+]?\d+/\d+(?=[ "()\'\n,;`])"#).token(T::LiteralNumber),
            rule(r#"[-+]?(\d*\.\d+([defls][-+]?\d+)?|\d+(\.\d*)?[defls][-+]?\d+)(?=[ "()\'\n,;`])"#).token(T::LiteralNumberFloat),
            rule(r#"#\\.(?=[ "()\'\n,;`])"#).token(T::LiteralStringChar),
            rule(r"#\\(\|[^|]+\||(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~])(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~]|[#.:])*)").token(T::LiteralStringChar),
            rule(r"#\(").token(T::Operator).push(&["body"]),
            rule(r"#\d*\*[01]*").token(T::LiteralOther),
            rule(r"#:(\|[^|]+\||(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~])(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~]|[#.:])*)").token(T::LiteralStringSymbol),
            rule(r"#[.,]").token(T::Operator),
            rule(r"#\'").token(T::NameFunction),
            rule(r"#b[+-]?[01]+(/[01]+)?").token(T::LiteralNumberBin),
            rule(r"#o[+-]?[0-7]+(/[0-7]+)?").token(T::LiteralNumberOct),
            rule(r"#x[+-]?[0-9a-f]+(/[0-9a-f]+)?").token(T::LiteralNumberHex),
            rule(r"#\d+r[+-]?[0-9a-z]+(/[0-9a-z]+)?").token(T::LiteralNumber),
            rule(r"(#c)(\()").groups(&[T::LiteralNumber, T::Punctuation]).push(&["body"]),
            rule(r"(#\d+a)(\()").groups(&[T::LiteralOther, T::Punctuation]).push(&["body"]),
            rule(r"(#s)(\()").groups(&[T::LiteralOther, T::Punctuation]).push(&["body"]),
            rule(r#"#p?"(\\.|[^"])*""#).token(T::LiteralOther),
            rule(r"#\d+=").token(T::Operator),
            rule(r"#\d+#").token(T::Operator),
            rule(r#"#+nil(?=[ "()\'\n,;`])\s*\("#).token(T::CommentPreproc).push(&["commented-form"]),
            rule(r"#[+-]").token(T::Operator),
            rule(r"(,@|,|\.)").token(T::Operator),
            rule(r#"(t|nil)(?=[ "()\'\n,;`])"#).token(T::NameConstant),
            rule(r"\*(\|[^|]+\||(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~])(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~]|[#.:])*)\*").token(T::NameVariableGlobal),
            rule(r"(\|[^|]+\||(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~])(?:\\.|[\w!$%&*+-/<=>?@\[\]^{}~]|[#.:])*)").token(T::NameVariable),
            rule(r"\(").token(T::Punctuation).push(&["body"]),
            rule(r"\)").token(T::Punctuation).pop(1),
        ]),
        ("root", &[
            rule("").push(&["body"]),
        ]),
        ("multiline-comment", &[
            rule(r"#\|").token(T::CommentMultiline).push(&[]),
            rule(r"\|#").token(T::CommentMultiline).pop(1),
            rule(r"[^|#]+").token(T::CommentMultiline),
            rule(r"[|#]").token(T::CommentMultiline),
        ]),
        ("commented-form", &[
            rule(r"\(").token(T::CommentPreproc).push(&[]),
            rule(r"\)").token(T::CommentPreproc).pop(1),
            rule(r"[^()]+").token(T::CommentPreproc),
        ]),
    ],
};
