//! Chroma's `groovy.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "groovy",
    config: ConfigDef {
        name: "Groovy",
        aliases: &["groovy"],
        filenames: &["*.groovy", "*.gradle"],
        mime_types: &["text/x-groovy"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"#!(.*?)$").token(T::CommentPreproc).push(&["base"]),
            rule("").push(&["base"]),
        ]),
        ("base", &[
            rule(r"^(\s*(?:[a-zA-Z_][\w.\[\]]*\s+)+?)([a-zA-Z_]\w*)(\s*)(\()").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::Token(T::Text), E::Token(T::Operator)]),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"@[a-zA-Z_][\w.]*").token(T::NameDecorator),
            rule(r"(as|assert|break|case|catch|continue|default|do|else|finally|for|if|in|goto|instanceof|new|return|switch|this|throw|try|while|in|as)\b").token(T::Keyword),
            rule(r"(abstract|const|extends|final|implements|native|private|protected|public|static|strictfp|super|synchronized|throws|transient|volatile)\b").token(T::KeywordDeclaration),
            rule(r"(def|var|boolean|byte|char|double|float|int|long|short|void)\b").token(T::KeywordType),
            rule(r"(package)(\s+)").groups(&[T::KeywordNamespace, T::Text]),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(class|interface|enum|trait|record)(\s+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["class"]),
            rule(r"(import)(\s+)").groups(&[T::KeywordNamespace, T::Text]).push(&["import"]),
            rule(r#"""".*?""""#).token(T::LiteralStringDouble),
            rule(r"'''.*?'''").token(T::LiteralStringSingle),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"\$/((?!/\$).)*/\$").token(T::LiteralString),
            rule(r#"/(\\\\|\\"|[^/])*/"#).token(T::LiteralString),
            rule(r"'\\.'|'[^\\]'|'\\u[0-9a-fA-F]{4}'").token(T::LiteralStringChar),
            rule(r"(\.)([a-zA-Z_]\w*)").groups(&[T::Operator, T::NameAttribute]),
            rule(r"[a-zA-Z_]\w*:").token(T::NameLabel),
            rule(r"[a-zA-Z_$]\w*").token(T::Name),
            rule(r"[~^*!%&\[\](){}<>|+=:;,./?-]").token(T::Operator),
            rule(r"[0-9][0-9]*\.[0-9]+([eE][0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+L?").token(T::LiteralNumberInteger),
            rule(r"\n").token(T::Text),
        ]),
        ("class", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameClass).pop(1),
        ]),
        ("import", &[
            rule(r"[\w.]+\*?").token(T::NameNamespace).pop(1),
        ]),
    ],
};
