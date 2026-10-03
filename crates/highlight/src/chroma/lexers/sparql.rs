//! Chroma's `sparql.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "sparql",
    config: ConfigDef {
        name: "SPARQL",
        aliases: &["sparql"],
        filenames: &["*.rq", "*.sparql"],
        mime_types: &["application/sparql-query"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string-escape", &[
            rule(r"u[0-9A-Fa-f]{4}").token(T::LiteralStringEscape).pop(1),
            rule(r"U[0-9A-Fa-f]{8}").token(T::LiteralStringEscape).pop(1),
            rule(r".").token(T::LiteralStringEscape).pop(1),
        ]),
        ("end-of-string", &[
            rule(r"(@)([a-zA-Z]+(?:-[a-zA-Z0-9]+)*)").groups(&[T::Operator, T::NameFunction]).pop(2),
            rule(r"\^\^").token(T::Operator).pop(2),
            rule("").pop(2),
        ]),
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"((?i)select|construct|describe|ask|where|filter|group\s+by|minus|distinct|reduced|from\s+named|from|order\s+by|desc|asc|limit|offset|bindings|load|clear|drop|create|add|move|copy|insert\s+data|delete\s+data|delete\s+where|delete|insert|using\s+named|using|graph|default|named|all|optional|service|silent|bind|union|not\s+in|in|as|having|to|prefix|base)\b").token(T::Keyword),
            rule(r"(a)\b").token(T::Keyword),
            rule(r#"(<(?:[^<>"{}|^`\\\x00-\x20])*>)"#).token(T::NameLabel),
            rule(r"(_:[_\p{L}\p{N}](?:[-_.\p{L}\p{N}]*[-_\p{L}\p{N}])?)").token(T::NameLabel),
            rule(r"[?$][_\p{L}\p{N}]+").token(T::NameVariable),
            rule(r#"([\p{L}][-_.\p{L}\p{N}]*)?(\:)((?:[_:\p{L}\p{N}]|(?:%[0-9A-Fa-f][0-9A-Fa-f])|(?:\\[ _~.\-!$&"()*+,;=/?#@%]))(?:(?:[-_:.\p{L}\p{N}]|(?:%[0-9A-Fa-f][0-9A-Fa-f])|(?:\\[ _~.\-!$&"()*+,;=/?#@%]))*(?:[-_:\p{L}\p{N}]|(?:%[0-9A-Fa-f][0-9A-Fa-f])|(?:\\[ _~.\-!$&"()*+,;=/?#@%])))?)?"#).groups(&[T::NameNamespace, T::Punctuation, T::NameTag]),
            rule(r"((?i)str|lang|langmatches|datatype|bound|iri|uri|bnode|rand|abs|ceil|floor|round|concat|strlen|ucase|lcase|encode_for_uri|contains|strstarts|strends|strbefore|strafter|year|month|day|hours|minutes|seconds|timezone|tz|now|md5|sha1|sha256|sha384|sha512|coalesce|if|strlang|strdt|sameterm|isiri|isuri|isblank|isliteral|isnumeric|regex|substr|replace|exists|not\s+exists|count|sum|min|max|avg|sample|group_concat|separator)\b").token(T::NameFunction),
            rule(r"(true|false)").token(T::KeywordConstant),
            rule(r"[+\-]?(\d+\.\d*[eE][+-]?\d+|\.?\d+[eE][+-]?\d+)").token(T::LiteralNumberFloat),
            rule(r"[+\-]?(\d+\.\d*|\.\d+)").token(T::LiteralNumberFloat),
            rule(r"[+\-]?\d+").token(T::LiteralNumberInteger),
            rule(r"(\|\||&&|=|\*|\-|\+|/|!=|<=|>=|!|<|>)").token(T::Operator),
            rule(r"[(){}.;,:^\[\]]").token(T::Punctuation),
            rule(r"#[^\n]*").token(T::Comment),
            rule(r#"""""#).token(T::LiteralString).push(&["triple-double-quoted-string"]),
            rule(r#"""#).token(T::LiteralString).push(&["single-double-quoted-string"]),
            rule(r"'''").token(T::LiteralString).push(&["triple-single-quoted-string"]),
            rule(r"'").token(T::LiteralString).push(&["single-single-quoted-string"]),
        ]),
        ("triple-double-quoted-string", &[
            rule(r#"""""#).token(T::LiteralString).push(&["end-of-string"]),
            rule(r"[^\\]+").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString).push(&["string-escape"]),
        ]),
        ("single-double-quoted-string", &[
            rule(r#"""#).token(T::LiteralString).push(&["end-of-string"]),
            rule(r#"[^"\\\n]+"#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralString).push(&["string-escape"]),
        ]),
        ("triple-single-quoted-string", &[
            rule(r"'''").token(T::LiteralString).push(&["end-of-string"]),
            rule(r"[^\\]+").token(T::LiteralString),
            rule(r"\\").token(T::LiteralStringEscape).push(&["string-escape"]),
        ]),
        ("single-single-quoted-string", &[
            rule(r"'").token(T::LiteralString).push(&["end-of-string"]),
            rule(r"[^'\\\n]+").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString).push(&["string-escape"]),
        ]),
    ],
};
