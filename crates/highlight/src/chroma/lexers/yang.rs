//! Chroma's `yang.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "yang",
    config: ConfigDef {
        name: "YANG",
        aliases: &["yang"],
        filenames: &["*.yang"],
        mime_types: &["application/yang"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"[\{\}\;]+").token(T::Punctuation),
            rule(r"(?<![\-\w])(and|or|not|\+|\.)(?![\-\w])").token(T::Operator),
            rule(r#""(?:\\"|[^"])*?""#).token(T::LiteralStringDouble),
            rule(r"'(?:\\'|[^'])*?'").token(T::LiteralStringSingle),
            rule(r"/\*").token(T::CommentMultiline).push(&["comments"]),
            rule(r"//.*?$").token(T::CommentSingle),
            rule(r"(?:^|(?<=[\s{};]))([\w.-]+)(:)([\w.-]+)(?=[\s{};])").groups(&[T::KeywordNamespace, T::Punctuation, T::Text]),
            rule(r"([0-9]{4}\-[0-9]{2}\-[0-9]{2})(?=[\s\{\}\;])").token(T::LiteralDate),
            rule(r"([0-9]+\.[0-9]+)(?=[\s\{\}\;])").token(T::LiteralNumberFloat),
            rule(r"([0-9]+)(?=[\s\{\}\;])").token(T::LiteralNumberInteger),
            rule(r"(submodule|module)(?=[^\w\-\:])").token(T::Keyword),
            rule(r"(yang-version|belongs-to|namespace|prefix)(?=[^\w\-\:])").token(T::Keyword),
            rule(r"(organization|description|reference|revision|contact)(?=[^\w\-\:])").token(T::Keyword),
            rule(r"(revision-date|include|import)(?=[^\w\-\:])").token(T::Keyword),
            rule(r"(notification|if-feature|deviation|extension|identity|argument|grouping|typedef|feature|augment|output|action|input|rpc)(?=[^\w\-\:])").token(T::Keyword),
            rule(r"(leaf-list|container|presence|anydata|deviate|choice|config|anyxml|refine|leaf|must|list|case|uses|when)(?=[^\w\-\:])").token(T::Keyword),
            rule(r"(require-instance|fraction-digits|error-app-tag|error-message|min-elements|max-elements|yin-element|ordered-by|position|modifier|default|pattern|length|status|units|value|range|type|path|enum|base|bit)(?=[^\w\-\:])").token(T::Keyword),
            rule(r"(mandatory|unique|key)(?=[^\w\-\:])").token(T::Keyword),
            rule(r"(not-supported|invert-match|deprecated|unbounded|obsolete|current|replace|delete|false|true|user|min|max|add)(?=[^\w\-\:])").token(T::NameClass),
            rule(r"(instance-identifier|identityref|enumeration|decimal64|boolean|leafref|uint64|uint32|string|binary|uint16|int32|int64|int16|empty|uint8|union|int8|bits)(?=[^\w\-\:])").token(T::NameClass),
            rule(r#"[^;{}\s\'\"]+"#).token(T::Text),
        ]),
        ("comments", &[
            rule(r"[^*/]").token(T::CommentMultiline),
            rule(r"/\*").token(T::CommentMultiline).push(&["comment"]),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"[*/]").token(T::CommentMultiline),
        ]),
    ],
};
