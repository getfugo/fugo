//! Chroma's `ceylon.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "ceylon",
    config: ConfigDef {
        name: "Ceylon",
        aliases: &["ceylon"],
        filenames: &["*.ceylon"],
        mime_types: &["text/x-ceylon"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("class", &[
            rule(r"[A-Za-z_]\w*").token(T::NameClass).pop(1),
        ]),
        ("import", &[
            rule(r"[a-z][\w.]*").token(T::NameNamespace).pop(1),
        ]),
        ("comment", &[
            rule(r"[^*/]").token(T::CommentMultiline),
            rule(r"/\*").token(T::CommentMultiline).push(&[]),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"[*/]").token(T::CommentMultiline),
        ]),
        ("root", &[
            rule(r"^(\s*(?:[a-zA-Z_][\w.\[\]]*\s+)+?)([a-zA-Z_]\w*)(\s*)(\()").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::Token(T::Text), E::Token(T::Operator)]),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*").token(T::CommentMultiline).push(&["comment"]),
            rule(r"(shared|abstract|formal|default|actual|variable|deprecated|small|late|literal|doc|by|see|throws|optional|license|tagged|final|native|annotation|sealed)\b").token(T::NameDecorator),
            rule(r"(break|case|catch|continue|else|finally|for|in|if|return|switch|this|throw|try|while|is|exists|dynamic|nonempty|then|outer|assert|let)\b").token(T::Keyword),
            rule(r"(abstracts|extends|satisfies|super|given|of|out|assign)\b").token(T::KeywordDeclaration),
            rule(r"(function|value|void|new)\b").token(T::KeywordType),
            rule(r"(assembly|module|package)(\s+)").groups(&[T::KeywordNamespace, T::Text]),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(class|interface|object|alias)(\s+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["class"]),
            rule(r"(import)(\s+)").groups(&[T::KeywordNamespace, T::Text]).push(&["import"]),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"'\\.'|'[^\\]'|'\\\{#[0-9a-fA-F]{4}\}'").token(T::LiteralStringChar),
            rule(r#"".*``.*``.*""#).token(T::LiteralStringInterpol),
            rule(r"(\.)([a-z_]\w*)").groups(&[T::Operator, T::NameAttribute]),
            rule(r"[a-zA-Z_]\w*:").token(T::NameLabel),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"[~^*!%&\[\](){}<>|+=:;,./?-]").token(T::Operator),
            rule(r"\d{1,3}(_\d{3})+\.\d{1,3}(_\d{3})+[kMGTPmunpf]?").token(T::LiteralNumberFloat),
            rule(r"\d{1,3}(_\d{3})+\.[0-9]+([eE][+-]?[0-9]+)?[kMGTPmunpf]?").token(T::LiteralNumberFloat),
            rule(r"[0-9][0-9]*\.\d{1,3}(_\d{3})+[kMGTPmunpf]?").token(T::LiteralNumberFloat),
            rule(r"[0-9][0-9]*\.[0-9]+([eE][+-]?[0-9]+)?[kMGTPmunpf]?").token(T::LiteralNumberFloat),
            rule(r"#([0-9a-fA-F]{4})(_[0-9a-fA-F]{4})+").token(T::LiteralNumberHex),
            rule(r"#[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"\$([01]{4})(_[01]{4})+").token(T::LiteralNumberBin),
            rule(r"\$[01]+").token(T::LiteralNumberBin),
            rule(r"\d{1,3}(_\d{3})+[kMGTP]?").token(T::LiteralNumberInteger),
            rule(r"[0-9]+[kMGTP]?").token(T::LiteralNumberInteger),
            rule(r"\n").token(T::Text),
        ]),
    ],
};
