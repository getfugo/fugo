//! Chroma's `sourcepawn.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "sourcepawn",
    config: ConfigDef {
        name: "SourcePawn",
        aliases: &["sp"],
        filenames: &["*.sp", "*.inc"],
        mime_types: &["text/x-sourcepawn"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^#if\s+0").token(T::CommentPreproc).push(&["if0"]),
            rule(r"^#").token(T::CommentPreproc).push(&["macro"]),
            rule(r"^\s*(?:/[*].*?[*]/\s*)*#if\s+0").token(T::CommentPreproc).push(&["if0"]),
            rule(r"^\s*(?:/[*].*?[*]/\s*)*#").token(T::CommentPreproc).push(&["macro"]),
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"/(\\\n)?/(\n|(.|\n)*?[^\\]\n)").token(T::CommentSingle),
            rule(r"/(\\\n)?\*(.|\n)*?\*(\\\n)?/").token(T::CommentMultiline),
            rule(r"[{}]").token(T::Punctuation),
            rule(r#"L?""#).token(T::LiteralString).push(&["string"]),
            rule(r"L?'(\\.|\\[0-7]{1,3}|\\x[a-fA-F0-9]{1,2}|[^\\\'\n])'").token(T::LiteralStringChar),
            rule(r"(\d+\.\d*|\.\d+|\d+)[eE][+-]?\d+[LlUu]*").token(T::LiteralNumberFloat),
            rule(r"(\d+\.\d*|\.\d+|\d+[fF])[fF]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+[LlUu]*").token(T::LiteralNumberHex),
            rule(r"0[0-7]+[LlUu]*").token(T::LiteralNumberOct),
            rule(r"\d+[LlUu]*").token(T::LiteralNumberInteger),
            rule(r"[~!%^&*+=|?:<>/-]").token(T::Operator),
            rule(r"[()\[\],.;]").token(T::Punctuation),
            rule(r"(case|const|continue|native|default|else|enum|for|if|new|operator|public|return|sizeof|static|decl|struct|switch)\b").token(T::Keyword),
            rule(r"(bool|float|void|int|char)\b").token(T::KeywordType),
            rule(r"(true|false)\b").token(T::KeywordConstant),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"((?:[\w*\s])+?(?:\s|[*]))([a-zA-Z_]\w*)(\s*\([^;]*?\))([^;{]*)(\{)").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::UsingSelf("root"), E::UsingSelf("root"), E::Token(T::Punctuation)]).push(&["function"]),
            rule(r"((?:[\w*\s])+?(?:\s|[*]))([a-zA-Z_]\w*)(\s*\([^;]*?\))([^;]*)(;)").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::UsingSelf("root"), E::UsingSelf("root"), E::Token(T::Punctuation)]),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-fA-F0-9]{2,4}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
        ("macro", &[
            rule(r"(include)(\s*(?:/[*].*?[*]/\s*)?)([^\n]+)").groups(&[T::CommentPreproc, T::Text, T::CommentPreprocFile]),
            rule(r"[^/\n]+").token(T::CommentPreproc),
            rule(r"/\*(.|\n)*?\*/").token(T::CommentMultiline),
            rule(r"//.*?\n").token(T::CommentSingle).pop(1),
            rule(r"/").token(T::CommentPreproc),
            rule(r"(?<=\\)\n").token(T::CommentPreproc),
            rule(r"\n").token(T::CommentPreproc).pop(1),
        ]),
        ("if0", &[
            rule(r"^\s*#if.*?(?<!\\)\n").token(T::CommentPreproc).push(&[]),
            rule(r"^\s*#endif.*?(?<!\\)\n").token(T::CommentPreproc).pop(1),
            rule(r".*?\n").token(T::Comment),
        ]),
    ],
};
