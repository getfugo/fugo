//! Chroma's `jsonnet.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "jsonnet",
    config: ConfigDef {
        name: "Jsonnet",
        aliases: &["jsonnet"],
        filenames: &["*.jsonnet", "*.libsonnet"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("_comments", &[
            rule(r"(//|#).*\n").token(T::CommentSingle),
            rule(r"/\*\*([^/]|/(?!\*))*\*/").token(T::LiteralStringDoc),
            rule(r"/\*([^/]|/(?!\*))*\*/").token(T::Comment),
        ]),
        ("root", &[
            include("_comments"),
            rule(r"@'.*'").token(T::LiteralString),
            rule(r#"@".*""#).token(T::LiteralString),
            rule(r"'").token(T::LiteralString).push(&["singlestring"]),
            rule(r#"""#).token(T::LiteralString).push(&["doublestring"]),
            rule(r"\|\|\|(.|\n)*\|\|\|").token(T::LiteralString),
            rule(r"[+-]?[0-9]+(.[0-9])?").token(T::LiteralNumberFloat),
            rule(r"[!$~+\-&|^=<>*/%]").token(T::Operator),
            rule(r"\{").token(T::Punctuation).push(&["object"]),
            rule(r"\[").token(T::Punctuation).push(&["array"]),
            rule(r"local\b").token(T::Keyword).push(&["local_name"]),
            rule(r"assert\b").token(T::Keyword).push(&["assert"]),
            rule(r"(assert|else|error|false|for|if|import|importstr|in|null|tailstrict|then|self|super|true)\b").token(T::Keyword),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"function(?=\()").token(T::Keyword).push(&["function_params"]),
            rule(r"std\.[^\W\d]\w*(?=\()").token(T::NameBuiltin).push(&["function_args"]),
            rule(r"[^\W\d]\w*(?=\()").token(T::NameFunction).push(&["function_args"]),
            rule(r"[^\W\d]\w*").token(T::NameVariable),
            rule(r"[\.()]").token(T::Punctuation),
        ]),
        ("singlestring", &[
            rule(r"[^'\\]").token(T::LiteralString),
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r"'").token(T::LiteralString).pop(1),
        ]),
        ("doublestring", &[
            rule(r#"[^"\\]"#).token(T::LiteralString),
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r#"""#).token(T::LiteralString).pop(1),
        ]),
        ("array", &[
            rule(r",").token(T::Punctuation),
            rule(r"\]").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("local_name", &[
            rule(r"[^\W\d]\w*(?=\()").token(T::NameFunction).push(&["function_params"]),
            rule(r"[^\W\d]\w*").token(T::NameVariable),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"(?==)").token(T::TextWhitespace).push(&["#pop", "local_value"]),
        ]),
        ("local_value", &[
            rule(r"=").token(T::Operator),
            rule(r";").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("assert", &[
            rule(r":").token(T::Punctuation),
            rule(r";").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("function_params", &[
            rule(r"[^\W\d]\w*").token(T::NameVariable),
            rule(r"\(").token(T::Punctuation),
            rule(r"\)").token(T::Punctuation).pop(1),
            rule(r",").token(T::Punctuation),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"=").token(T::Operator).push(&["function_param_default"]),
        ]),
        ("function_args", &[
            rule(r"\(").token(T::Punctuation),
            rule(r"\)").token(T::Punctuation).pop(1),
            rule(r",").token(T::Punctuation),
            rule(r"\s+").token(T::TextWhitespace),
            include("root"),
        ]),
        ("object", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"local\b").token(T::Keyword).push(&["object_local_name"]),
            rule(r"assert\b").token(T::Keyword).push(&["object_assert"]),
            rule(r"\[").token(T::Operator).push(&["field_name_expr"]),
            rule(r"(?=[^\W\d]\w*)").token(T::Text).push(&["field_name"]),
            rule(r"\}").token(T::Punctuation).pop(1),
            rule(r#"""#).token(T::NameVariable).push(&["double_field_name"]),
            rule(r"'").token(T::NameVariable).push(&["single_field_name"]),
            include("_comments"),
        ]),
        ("field_name", &[
            rule(r"[^\W\d]\w*(?=\()").token(T::NameFunction).push(&["field_separator", "function_params"]),
            rule(r"[^\W\d]\w*").token(T::NameVariable).push(&["field_separator"]),
        ]),
        ("double_field_name", &[
            rule(r#"([^"\\]|\\.)*""#).token(T::NameVariable).push(&["field_separator"]),
        ]),
        ("single_field_name", &[
            rule(r"([^'\\]|\\.)*'").token(T::NameVariable).push(&["field_separator"]),
        ]),
        ("field_name_expr", &[
            rule(r"\]").token(T::Operator).push(&["field_separator"]),
            include("root"),
        ]),
        ("function_param_default", &[
            rule(r"(?=[,\)])").token(T::TextWhitespace).pop(1),
            include("root"),
        ]),
        ("field_separator", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"\+?::?:?").token(T::Punctuation).push(&["#pop", "#pop", "field_value"]),
            include("_comments"),
        ]),
        ("field_value", &[
            rule(r",").token(T::Punctuation).pop(1),
            rule(r"\}").token(T::Punctuation).pop(2),
            include("root"),
        ]),
        ("object_assert", &[
            rule(r":").token(T::Punctuation),
            rule(r",").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("object_local_name", &[
            rule(r"[^\W\d]\w*").token(T::NameVariable).push(&["#pop", "object_local_value"]),
            rule(r"\s+").token(T::TextWhitespace),
        ]),
        ("object_local_value", &[
            rule(r"=").token(T::Operator),
            rule(r",").token(T::Punctuation).pop(1),
            rule(r"\}").token(T::Punctuation).pop(2),
            include("root"),
        ]),
    ],
};
