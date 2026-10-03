//! Chroma's `kotlin.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "kotlin",
    config: ConfigDef {
        name: "Kotlin",
        aliases: &["kotlin"],
        filenames: &["*.kt", "*.kts"],
        mime_types: &["text/x-kotlin"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#"\\[tbnr'"\\\$]"#).token(T::LiteralStringEscape),
            rule(r"\\u[0-9a-fA-F]{4}").token(T::LiteralStringEscape),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            include("string-interpol"),
            rule(r#"[^\n\\"$]+"#).token(T::LiteralStringDouble),
            rule(r"\$").token(T::LiteralStringDouble),
        ]),
        ("package", &[
            rule(r"\S+").token(T::NameNamespace).pop(1),
        ]),
        ("class", &[
            rule(r"\x60[^\x60]+?\x60").token(T::NameClass).pop(1),
            rule(r"(?:[_\p{L}][\p{L}\p{N}]*|`@?[_\p{L}][\p{L}\p{N}]+`)").token(T::NameClass).pop(1),
        ]),
        ("property", &[
            rule(r"\x60[^\x60]+?\x60").token(T::NameProperty).pop(1),
            rule(r"(?:[_\p{L}][\p{L}\p{N}]*|`@?[_\p{L}][\p{L}\p{N}]+`)").token(T::NameProperty).pop(1),
        ]),
        ("string-interpol", &[
            rule(r"\$(?:[_\p{L}][\p{L}\p{N}]*|`@?[_\p{L}][\p{L}\p{N}]+`)").token(T::LiteralStringInterpol),
            rule(r"\${[^}\n]*}").token(T::LiteralStringInterpol),
        ]),
        ("generics-specification", &[
            rule(r"<").token(T::Punctuation).push(&["generics-specification"]),
            rule(r">").token(T::Punctuation).pop(1),
            rule(r"[,:*?]").token(T::Punctuation),
            rule(r"(in|out|reified)").token(T::Keyword),
            rule(r"\x60[^\x60]+?\x60").token(T::NameClass),
            rule(r"(?:[_\p{L}][\p{L}\p{N}]*|`@?[_\p{L}][\p{L}\p{N}]+`)").token(T::NameClass),
            rule(r"\s+").token(T::Text),
        ]),
        ("root", &[
            rule(r"^\s*\[.*?\]").token(T::NameAttribute),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"//[^\n]*\n?").token(T::CommentSingle),
            rule(r"/[*].*?[*]/").token(T::CommentMultiline),
            rule(r"\n").token(T::Text),
            rule(r"!==|!in|!is|===").token(T::Operator),
            rule(r"%=|&&|\*=|\+\+|\+=|--|-=|->|\.\.|\/=|::|<=|==|>=|!!|!=|\|\||\?[:.]").token(T::Operator),
            rule(r"[~!%^&*()+=|\[\]:;,.<>\/?-]").token(T::Punctuation),
            rule(r"[{}]").token(T::Punctuation),
            rule(r#"""""#).token(T::LiteralString).push(&["rawstring"]),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"(')(\\u[0-9a-fA-F]{4})(')").groups(&[T::LiteralStringChar, T::LiteralStringEscape, T::LiteralStringChar]),
            rule(r"'\\.'|'[^\\]'").token(T::LiteralStringChar),
            rule(r"0[xX][0-9a-fA-F]+[Uu]?[Ll]?|[0-9]+(\.[0-9]*)?([eE][+-][0-9]+)?[fF]?[Uu]?[Ll]?").token(T::LiteralNumber),
            rule(r"(companion)(\s+)(object)").groups(&[T::Keyword, T::Text, T::Keyword]),
            rule(r"(class|interface|object)(\s+)").groups(&[T::Keyword, T::Text]).push(&["class"]),
            rule(r"(package|import)(\s+)").groups(&[T::Keyword, T::Text]).push(&["package"]),
            rule(r"(val|var)(\s+)").groups(&[T::Keyword, T::Text]).push(&["property"]),
            rule(r"(fun)(\s+)").groups(&[T::Keyword, T::Text]).push(&["function"]),
            rule(r"(abstract|actual|annotation|as|as\?|break|by|catch|class|companion|const|constructor|continue|crossinline|data|delegate|do|dynamic|else|enum|expect|external|false|field|file|final|finally|for|fun|get|if|import|in|infix|init|inline|inner|interface|internal|is|it|lateinit|noinline|null|object|open|operator|out|override|package|param|private|property|protected|public|receiver|reified|return|sealed|set|setparam|super|suspend|tailrec|this|throw|true|try|typealias|typeof|val|value|var|vararg|when|where|while)\b").token(T::Keyword),
            rule(r"@(?:[_\p{L}][\p{L}\p{N}]*|`@?[_\p{L}][\p{L}\p{N}]+`)").token(T::NameDecorator),
            rule(r"(?:\p{Lu}[_\p{L}]*)(?=\.)").token(T::NameClass),
            rule(r"(?:[_\p{L}][\p{L}\p{N}]*|`@?[_\p{L}][\p{L}\p{N}]+`)").token(T::Name),
        ]),
        ("function", &[
            rule(r"<").token(T::Punctuation).push(&["generics-specification"]),
            rule(r"\x60[^\x60]+?\x60").token(T::NameFunction).pop(1),
            rule(r"(?:[_\p{L}][\p{L}\p{N}]*|`@?[_\p{L}][\p{L}\p{N}]+`)").token(T::NameFunction).pop(1),
            rule(r"\s+").token(T::Text),
        ]),
        ("rawstring", &[
            rule(r#"""""#).token(T::LiteralString).pop(1),
            rule(r#"(?:[^$"]+|\"{1,2}[^"])+"#).token(T::LiteralString),
            include("string-interpol"),
            rule(r"\$").token(T::LiteralString),
        ]),
    ],
};
