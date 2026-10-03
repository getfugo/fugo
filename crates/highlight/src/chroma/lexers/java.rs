//! Chroma's `java.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "java",
    config: ConfigDef {
        name: "Java",
        aliases: &["java"],
        filenames: &["*.java"],
        mime_types: &["text/x-java"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(^\s*)((?:(?:public|private|protected|static|strictfp)(?:\s+))*)(record)\b").bygroups(&[E::Token(T::TextWhitespace), E::UsingSelf("root"), E::Token(T::KeywordDeclaration)]).push(&["class"]),
            rule(r"[^\S\n]+").token(T::TextWhitespace),
            rule(r"(//.*?)(\n)").groups(&[T::CommentSingle, T::TextWhitespace]),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"(assert|break|case|catch|continue|default|do|else|finally|for|if|goto|instanceof|new|return|switch|this|throw|try|while)\b").token(T::Keyword),
            rule(r"((?:(?:[^\W\d]|\$)[\w.\[\]$<>]*\s+)+?)((?:[^\W\d]|\$)[\w$]*)(\s*)(\()").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::Token(T::TextWhitespace), E::Token(T::Punctuation)]),
            rule(r"@[^\W\d][\w.]*").token(T::NameDecorator),
            rule(r"(abstract|const|enum|extends|final|implements|native|private|protected|public|sealed|static|strictfp|super|synchronized|throws|transient|volatile|yield)\b").token(T::KeywordDeclaration),
            rule(r"(boolean|byte|char|double|float|int|long|short|void)\b").token(T::KeywordType),
            rule(r"(package)(\s+)").groups(&[T::KeywordNamespace, T::TextWhitespace]).push(&["import"]),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(class|interface)\b").token(T::KeywordDeclaration).push(&["class"]),
            rule(r"(var)(\s+)").groups(&[T::KeywordDeclaration, T::TextWhitespace]).push(&["var"]),
            rule(r"(import(?:\s+static)?)(\s+)").groups(&[T::KeywordNamespace, T::TextWhitespace]).push(&["import"]),
            rule(r#""""\n"#).token(T::LiteralString).push(&["multiline_string"]),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"'\\.'|'[^\\]'|'\\u[0-9a-fA-F]{4}'").token(T::LiteralStringChar),
            rule(r"(\.)((?:[^\W\d]|\$)[\w$]*)").groups(&[T::Punctuation, T::NameAttribute]),
            rule(r"^(\s*)(default)(:)").groups(&[T::TextWhitespace, T::Keyword, T::Punctuation]),
            rule(r"^(\s*)((?:[^\W\d]|\$)[\w$]*)(:)").groups(&[T::TextWhitespace, T::NameLabel, T::Punctuation]),
            rule(r"([^\W\d]|\$)[\w$]*").token(T::Name),
            rule(r"([0-9][0-9_]*\.([0-9][0-9_]*)?|\.[0-9][0-9_]*)([eE][+\-]?[0-9][0-9_]*)?[fFdD]?|[0-9][eE][+\-]?[0-9][0-9_]*[fFdD]?|[0-9]([eE][+\-]?[0-9][0-9_]*)?[fFdD]|0[xX]([0-9a-fA-F][0-9a-fA-F_]*\.?|([0-9a-fA-F][0-9a-fA-F_]*)?\.[0-9a-fA-F][0-9a-fA-F_]*)[pP][+\-]?[0-9][0-9_]*[fFdD]?").token(T::LiteralNumberFloat),
            rule(r"0[xX][0-9a-fA-F][0-9a-fA-F_]*[lL]?").token(T::LiteralNumberHex),
            rule(r"0[bB][01][01_]*[lL]?").token(T::LiteralNumberBin),
            rule(r"0[0-7_]+[lL]?").token(T::LiteralNumberOct),
            rule(r"0|[1-9][0-9_]*[lL]?").token(T::LiteralNumberInteger),
            rule(r"[~^*!%&\[\]<>|+=/?-]").token(T::Operator),
            rule(r"[{}();:.,]").token(T::Punctuation),
            rule(r"\n").token(T::TextWhitespace),
        ]),
        ("class", &[
            rule(r"\s+").token(T::Text),
            rule(r"([^\W\d]|\$)[\w$]*").token(T::NameClass).pop(1),
        ]),
        ("var", &[
            rule(r"([^\W\d]|\$)[\w$]*").token(T::Name).pop(1),
        ]),
        ("import", &[
            rule(r"[\w.]+\*?").token(T::NameNamespace).pop(1),
        ]),
        ("multiline_string", &[
            rule(r#"""""#).token(T::LiteralString).pop(1),
            rule(r#"""#).token(T::LiteralString),
            include("string"),
        ]),
        ("string", &[
            rule(r#"[^\\"]+"#).token(T::LiteralString),
            rule(r"\\\\").token(T::LiteralString),
            rule(r#"\\""#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
            rule(r#"""#).token(T::LiteralString).pop(1),
        ]),
    ],
};
