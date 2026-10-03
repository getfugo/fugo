//! Chroma's `emacslisp.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "emacslisp",
    config: ConfigDef {
        name: "EmacsLisp",
        aliases: &["emacs", "elisp", "emacs-lisp"],
        filenames: &["*.el"],
        mime_types: &["text/x-elisp", "application/x-elisp"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#"[^"\\`]+"#).token(T::LiteralString),
            rule(r"`((?:\\.|[\w!$%&*+-/<=>?@^{}~|])(?:\\.|[\w!$%&*+-/<=>?@^{}~|]|[#.:])*)\'").token(T::LiteralStringSymbol),
            rule(r"`").token(T::LiteralString),
            rule(r"\\.").token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r#"""#).token(T::LiteralString).pop(1),
        ]),
        ("root", &[
            rule("").push(&["body"]),
        ]),
        ("body", &[
            rule(r"\s+").token(T::Text),
            rule(r";.*$").token(T::CommentSingle),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"\?([^\\]|\\.)").token(T::LiteralStringChar),
            rule(r":((?:\\.|[\w!$%&*+-/<=>?@^{}~|])(?:\\.|[\w!$%&*+-/<=>?@^{}~|]|[#.:])*)").token(T::NameBuiltin),
            rule(r"::((?:\\.|[\w!$%&*+-/<=>?@^{}~|])(?:\\.|[\w!$%&*+-/<=>?@^{}~|]|[#.:])*)").token(T::LiteralStringSymbol),
            rule(r"'((?:\\.|[\w!$%&*+-/<=>?@^{}~|])(?:\\.|[\w!$%&*+-/<=>?@^{}~|]|[#.:])*)").token(T::LiteralStringSymbol),
            rule(r"'").token(T::Operator),
            rule(r"`").token(T::Operator),
            rule(r#"[-+]?\d+\.?(?=[ "()\]\'\n,;`])"#).token(T::LiteralNumberInteger),
            rule(r#"[-+]?\d+/\d+(?=[ "()\]\'\n,;`])"#).token(T::LiteralNumber),
            rule(r#"[-+]?(\d*\.\d+([defls][-+]?\d+)?|\d+(\.\d*)?[defls][-+]?\d+)(?=[ "()\]\'\n,;`])"#).token(T::LiteralNumberFloat),
            rule(r"\[|\]").token(T::Punctuation),
            rule(r"#:((?:\\.|[\w!$%&*+-/<=>?@^{}~|])(?:\\.|[\w!$%&*+-/<=>?@^{}~|]|[#.:])*)").token(T::LiteralStringSymbol),
            rule(r"#\^\^?").token(T::Operator),
            rule(r"#\'").token(T::NameFunction),
            rule(r"#[bB][+-]?[01]+(/[01]+)?").token(T::LiteralNumberBin),
            rule(r"#[oO][+-]?[0-7]+(/[0-7]+)?").token(T::LiteralNumberOct),
            rule(r"#[xX][+-]?[0-9a-fA-F]+(/[0-9a-fA-F]+)?").token(T::LiteralNumberHex),
            rule(r"#\d+r[+-]?[0-9a-zA-Z]+(/[0-9a-zA-Z]+)?").token(T::LiteralNumber),
            rule(r"#\d+=").token(T::Operator),
            rule(r"#\d+#").token(T::Operator),
            rule(r"(,@|,|\.|:)").token(T::Operator),
            rule(r#"(t|nil)(?=[ "()\]\'\n,;`])"#).token(T::NameConstant),
            rule(r"\*((?:\\.|[\w!$%&*+-/<=>?@^{}~|])(?:\\.|[\w!$%&*+-/<=>?@^{}~|]|[#.:])*)\*").token(T::NameVariableGlobal),
            rule(r"((?:\\.|[\w!$%&*+-/<=>?@^{}~|])(?:\\.|[\w!$%&*+-/<=>?@^{}~|]|[#.:])*)").token(T::NameVariable),
            rule(r"#\(").token(T::Operator).push(&["body"]),
            rule(r"\(").token(T::Punctuation).push(&["body"]),
            rule(r"\)").token(T::Punctuation).pop(1),
        ]),
    ],
};
