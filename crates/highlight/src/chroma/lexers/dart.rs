//! Chroma's `dart.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "dart",
    config: ConfigDef {
        name: "Dart",
        aliases: &["dart"],
        filenames: &["*.dart"],
        mime_types: &["text/x-dart"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string_double_multiline", &[
            rule(r#"""""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"[^"$\\]+"#).token(T::LiteralStringDouble),
            include("string_common"),
            rule(r#"(\$|\")+"#).token(T::LiteralStringDouble),
        ]),
        ("class", &[
            rule(r"[a-zA-Z_$]\w*").token(T::NameClass).pop(1),
        ]),
        ("import_decl", &[
            include("string_literal"),
            rule(r"\s+").token(T::Text),
            rule(r"\b(as|show|hide)\b").token(T::Keyword),
            rule(r"[a-zA-Z_$]\w*").token(T::Name),
            rule(r"\,").token(T::Punctuation),
            rule(r"\;").token(T::Punctuation).pop(1),
        ]),
        ("string_single_multiline", &[
            rule(r"'''").token(T::LiteralStringSingle).pop(1),
            rule(r"[^\'$\\]+").token(T::LiteralStringSingle),
            include("string_common"),
            rule(r"(\$|\')+").token(T::LiteralStringSingle),
        ]),
        ("root", &[
            include("string_literal"),
            rule(r"#!(.*?)$").token(T::CommentPreproc),
            rule(r"\b(import|export)\b").token(T::Keyword).push(&["import_decl"]),
            rule(r"\b(library|source|part of|part)\b").token(T::Keyword),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"\b(class)\b(\s+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["class"]),
            rule(r"\b(assert|break|case|catch|continue|default|do|else|finally|for|if|in|is|new|return|super|switch|this|throw|try|while)\b").token(T::Keyword),
            rule(r"\b(abstract|async|await|const|extends|factory|final|get|implements|native|operator|required|set|static|sync|typedef|var|with|yield)\b").token(T::KeywordDeclaration),
            rule(r"\b(bool|double|dynamic|int|num|Object|String|void)\b").token(T::KeywordType),
            rule(r"\b(false|null|true)\b").token(T::KeywordConstant),
            rule(r"[~!%^&*+=|?:<>/-]|as\b").token(T::Operator),
            rule(r"[a-zA-Z_$]\w*:").token(T::NameLabel),
            rule(r"[a-zA-Z_$]\w*").token(T::Name),
            rule(r"[(){}\[\],.;]").token(T::Punctuation),
            rule(r"0[xX][0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"\d+(\.\d*)?([eE][+-]?\d+)?").token(T::LiteralNumber),
            rule(r"\.\d+([eE][+-]?\d+)?").token(T::LiteralNumber),
            rule(r"\n").token(T::Text),
        ]),
        ("string_literal", &[
            rule(r#"r"""([\w\W]*?)""""#).token(T::LiteralStringDouble),
            rule(r"r'''([\w\W]*?)'''").token(T::LiteralStringSingle),
            rule(r#"r"(.*?)""#).token(T::LiteralStringDouble),
            rule(r"r'(.*?)'").token(T::LiteralStringSingle),
            rule(r#"""""#).token(T::LiteralStringDouble).push(&["string_double_multiline"]),
            rule(r"'''").token(T::LiteralStringSingle).push(&["string_single_multiline"]),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string_double"]),
            rule(r"'").token(T::LiteralStringSingle).push(&["string_single"]),
        ]),
        ("string_common", &[
            rule(r#"\\(x[0-9A-Fa-f]{2}|u[0-9A-Fa-f]{4}|u\{[0-9A-Fa-f]*\}|[a-z'\"$\\])"#).token(T::LiteralStringEscape),
            rule(r"(\$)([a-zA-Z_]\w*)").groups(&[T::LiteralStringInterpol, T::Name]),
            rule(r"(\$\{)(.*?)(\})").bygroups(&[E::Token(T::LiteralStringInterpol), E::UsingSelf("root"), E::Token(T::LiteralStringInterpol)]),
        ]),
        ("string_double", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"[^"$\\\n]+"#).token(T::LiteralStringDouble),
            include("string_common"),
            rule(r"\$+").token(T::LiteralStringDouble),
        ]),
        ("string_single", &[
            rule(r"'").token(T::LiteralStringSingle).pop(1),
            rule(r"[^'$\\\n]+").token(T::LiteralStringSingle),
            include("string_common"),
            rule(r"\$+").token(T::LiteralStringSingle),
        ]),
    ],
};
