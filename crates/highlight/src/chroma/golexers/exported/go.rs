//! Chroma's `go.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "go",
    config: ConfigDef {
        name: "Go",
        aliases: &["go", "golang"],
        filenames: &["*.go"],
        mime_types: &["text/x-gosrc"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"//[^\n\r]*").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"(import|package)\b").token(T::KeywordNamespace),
            rule(r"(var|func|struct|map|chan|type|interface|const)\b").token(T::KeywordDeclaration),
            rule(r"(fallthrough|continue|default|select|return|switch|break|defer|range|case|goto|else|for|if|go)\b").token(T::Keyword),
            rule(r"(true|false|iota|nil)\b").token(T::KeywordConstant),
            rule(r"(complex128|complex64|float32|println|uintptr|complex|recover|float64|delete|uint64|uint16|string|uint32|append|int16|clear|int64|int32|close|error|float|print|uint8|panic|bool|byte|rune|real|imag|make|int8|uint|copy|cap|new|len|int|min|max)\b(\()").groups(&[T::NameBuiltin, T::Punctuation]),
            rule(r"(complex128|complex64|float32|uintptr|float64|uint16|uint32|uint64|string|error|float|int64|int32|int16|uint8|byte|rune|uint|bool|int8|int|any)\b").token(T::KeywordType),
            rule(r"\d+i").token(T::LiteralNumber),
            rule(r"\d+\.\d*([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\.\d+([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\d+[Ee][-+]\d+i").token(T::LiteralNumber),
            rule(r"\d+(\.\d+[eE][+\-]?\d+|\.\d*|[eE][+\-]?\d+)").token(T::LiteralNumberFloat),
            rule(r"\.\d+([eE][+\-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"0[0-7]+").token(T::LiteralNumberOct),
            rule(r"0[xX][0-9a-fA-F_]+").token(T::LiteralNumberHex),
            rule(r"0b[01_]+").token(T::LiteralNumberBin),
            rule(r"(0|[1-9][0-9_]*)").token(T::LiteralNumberInteger),
            rule(r#"'(\\['"\\abfnrtv]|\\x[0-9a-fA-F]{2}|\\[0-7]{1,3}|\\u[0-9a-fA-F]{4}|\\U[0-9a-fA-F]{8}|[^\\])'"#).token(T::LiteralStringChar),
            rule(r"(`)([^`]*)(`)").bygroups(&[E::Token(T::LiteralString), E::Func("goTextTemplateString"), E::Token(T::LiteralString)]),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"(<<=|>>=|<<|>>|<=|>=|&\^=|&\^|\+=|-=|\*=|/=|%=|&=|\|=|&&|\|\||<-|\+\+|--|==|!=|:=|\.\.\.|[+\-*/%&])").token(T::Operator),
            rule(r"([a-zA-Z_]\w*)(\s*)(\()").bygroups(&[E::Token(T::NameFunction), E::UsingSelf("root"), E::Token(T::Punctuation)]),
            rule(r"[|^<>=!()\[\]{}.,;:~]").token(T::Punctuation),
            rule(r"[^\W\d]\w*").token(T::NameOther),
        ]),
    ],
};
