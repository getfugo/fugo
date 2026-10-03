//! Chroma's `go_template.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "go_template",
    config: ConfigDef {
        name: "Go Template",
        aliases: &["go-template"],
        filenames: &["*.gotmpl", "*.go.tmpl"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("template", &[
            rule(r"[-]?}}").token(T::CommentPreproc).pop(1),
            rule(r"(?=}})").token(T::CommentPreproc).pop(1),
            rule(r"\(").token(T::Operator).push(&["subexpression"]),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            include("expression"),
        ]),
        ("subexpression", &[
            rule(r"\)").token(T::Operator).pop(1),
            include("expression"),
        ]),
        ("expression", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"\(").token(T::Operator).push(&["subexpression"]),
            rule(r"(range|if|else|while|with|template|end|true|false|nil|and|call|html|index|js|len|not|or|print|printf|println|urlquery|eq|ne|lt|le|gt|ge|block|break|continue|define|slice)\b").token(T::Keyword),
            rule(r"\||:?=|,").token(T::Operator),
            rule(r"[$]?[^\W\d]\w*").token(T::NameOther),
            rule(r"\$|[$]?\.(?:[^\W\d]\w*)?").token(T::NameAttribute),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"-?\d+i").token(T::LiteralNumber),
            rule(r"-?\d+\.\d*([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\.\d+([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"-?\d+[Ee][-+]\d+i").token(T::LiteralNumber),
            rule(r"-?\d+(\.\d+[eE][+\-]?\d+|\.\d*|[eE][+\-]?\d+)").token(T::LiteralNumberFloat),
            rule(r"-?\.\d+([eE][+\-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"-?0[0-7]+").token(T::LiteralNumberOct),
            rule(r"-?0[xX][0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"-?0b[01_]+").token(T::LiteralNumberBin),
            rule(r"-?(0|[1-9][0-9]*)").token(T::LiteralNumberInteger),
            rule(r#"'(\\['"\\abfnrtv]|\\x[0-9a-fA-F]{2}|\\[0-7]{1,3}|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}|[^\\])'"#).token(T::LiteralStringChar),
            rule(r"`[^`]*`").token(T::LiteralString),
        ]),
        ("root", &[
            rule(r"{{(- )?/\*(.|\n)*?\*/( -)?}}").token(T::CommentMultiline),
            rule(r"{{[-]?").token(T::CommentPreproc).push(&["template"]),
            rule(r"[^{]+").token(T::Other),
            rule(r"{").token(T::Other),
        ]),
    ],
};
