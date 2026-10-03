//! Chroma's `thrift.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "thrift",
    config: ConfigDef {
        name: "Thrift",
        aliases: &["thrift"],
        filenames: &["*.thrift"],
        mime_types: &["application/x-thrift"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("class", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameClass).pop(1),
            rule("").pop(1),
        ]),
        ("keywords", &[
            rule(r"(async|oneway|extends|throws|required|optional)\b").token(T::Keyword),
            rule(r"(true|false)\b").token(T::KeywordConstant),
            rule(r"(const|typedef)\b").token(T::KeywordDeclaration),
            rule(r"(smalltalk_category|smalltalk_prefix|delphi_namespace|csharp_namespace|ruby_namespace|xsd_namespace|cpp_namespace|php_namespace|xsd_nillable|xsd_optional|java_package|cocoa_prefix|perl_package|cpp_include|py_module|xsd_attrs|cpp_type|xsd_all|include)\b").token(T::KeywordNamespace),
            rule(r"(double|binary|string|slist|senum|bool|void|byte|list|i64|map|set|i32|i16)\b").token(T::KeywordType),
            rule(r"\b(__NAMESPACE__|synchronized|__FUNCTION__|__METHOD__|endforeach|implements|enddeclare|instanceof|transient|endswitch|protected|interface|__CLASS__|continue|__FILE__|abstract|function|endwhile|unsigned|register|volatile|__LINE__|declare|foreach|default|__DIR__|private|finally|dynamic|virtual|lambda|elseif|inline|switch|unless|endfor|delete|import|return|module|ensure|native|rescue|assert|sizeof|static|global|except|public|float|BEGIN|super|endif|yield|elsif|throw|clone|class|catch|until|break|retry|begin|raise|alias|while|print|undef|exec|with|when|case|redo|args|elif|this|then|self|goto|else|pass|next|var|for|xor|END|not|try|del|and|def|new|use|nil|end|if|do|is|or|in|as)\b").token(T::KeywordReserved),
        ]),
        ("numbers", &[
            rule(r"[+-]?(\d+\.\d+([eE][+-]?\d+)?|\.?\d+[eE][+-]?\d+)").token(T::LiteralNumberFloat),
            rule(r"[+-]?0x[0-9A-Fa-f]+").token(T::LiteralNumberHex),
            rule(r"[+-]?[0-9]+").token(T::LiteralNumberInteger),
        ]),
        ("root", &[
            include("whitespace"),
            include("comments"),
            rule(r#"""#).token(T::LiteralStringDouble).combined(&["stringescape", "dqs"]),
            rule(r"\'").token(T::LiteralStringSingle).combined(&["stringescape", "sqs"]),
            rule(r"(namespace)(\s+)").groups(&[T::KeywordNamespace, T::TextWhitespace]).push(&["namespace"]),
            rule(r"(enum|union|struct|service|exception)(\s+)").groups(&[T::KeywordDeclaration, T::TextWhitespace]).push(&["class"]),
            rule(r"((?:(?:[^\W\d]|\$)[\w.\[\]$<>]*\s+)+?)((?:[^\W\d]|\$)[\w$]*)(\s*)(\()").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::Token(T::Text), E::Token(T::Operator)]),
            include("keywords"),
            include("numbers"),
            rule(r"[&=]").token(T::Operator),
            rule(r"[:;,{}()<>\[\]]").token(T::Punctuation),
            rule(r"[a-zA-Z_](\.\w|\w)*").token(T::Name),
        ]),
        ("dqs", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"[^\\"\n]+"#).token(T::LiteralStringDouble),
        ]),
        ("namespace", &[
            rule(r"[a-z*](\.\w|\w)*").token(T::NameNamespace).pop(1),
            rule("").pop(1),
        ]),
        ("whitespace", &[
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\s+").token(T::TextWhitespace),
        ]),
        ("comments", &[
            rule(r"#.*$").token(T::Comment),
            rule(r"//.*?\n").token(T::Comment),
            rule(r"/\*[\w\W]*?\*/").token(T::CommentMultiline),
        ]),
        ("stringescape", &[
            rule(r#"\\([\\nrt"\'])"#).token(T::LiteralStringEscape),
        ]),
        ("sqs", &[
            rule(r"'").token(T::LiteralStringSingle).pop(1),
            rule(r"[^\\\'\n]+").token(T::LiteralStringSingle),
        ]),
    ],
};
