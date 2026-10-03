//! Chroma's `vala.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "vala",
    config: ConfigDef {
        name: "Vala",
        aliases: &["vala", "vapi"],
        filenames: &["*.vala", "*.vapi"],
        mime_types: &["text/x-vala"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("whitespace", &[
            rule(r"^\s*#if\s+0").token(T::CommentPreproc).push(&["if0"]),
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"\\\n").token(T::Text),
            rule(r"//(\n|(.|\n)*?[^\\]\n)").token(T::CommentSingle),
            rule(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/").token(T::CommentMultiline),
        ]),
        ("statements", &[
            rule(r#"[L@]?""#).token(T::LiteralString).push(&["string"]),
            rule(r"L?'(\\.|\\[0-7]{1,3}|\\x[a-fA-F0-9]{1,2}|[^\\\'\n])'").token(T::LiteralStringChar),
            rule(r#"(?s)""".*?""""#).token(T::LiteralString),
            rule(r"(\d+\.\d*|\.\d+|\d+)[eE][+-]?\d+[lL]?").token(T::LiteralNumberFloat),
            rule(r"(\d+\.\d*|\.\d+|\d+[fF])[fF]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+[Ll]?").token(T::LiteralNumberHex),
            rule(r"0[0-7]+[Ll]?").token(T::LiteralNumberOct),
            rule(r"\d+[Ll]?").token(T::LiteralNumberInteger),
            rule(r"[~!%^&*+=|?:<>/-]").token(T::Operator),
            rule(r"(\[)(Compact|Immutable|(?:Boolean|Simple)Type)(\])").groups(&[T::Punctuation, T::NameDecorator, T::Punctuation]),
            rule(r"(\[)(CCode|(?:Integer|Floating)Type)").groups(&[T::Punctuation, T::NameDecorator]),
            rule(r"[()\[\],.]").token(T::Punctuation),
            rule(r"(as|base|break|case|catch|construct|continue|default|delete|do|else|enum|finally|for|foreach|get|if|in|is|lock|new|out|params|return|set|sizeof|switch|this|throw|try|typeof|while|yield)\b").token(T::Keyword),
            rule(r"(abstract|const|delegate|dynamic|ensures|extern|inline|internal|override|owned|private|protected|public|ref|requires|signal|static|throws|unowned|var|virtual|volatile|weak|yields)\b").token(T::KeywordDeclaration),
            rule(r"(namespace|using)(\s+)").groups(&[T::KeywordNamespace, T::TextWhitespace]).push(&["namespace"]),
            rule(r"(class|errordomain|interface|struct)(\s+)").groups(&[T::KeywordDeclaration, T::TextWhitespace]).push(&["class"]),
            rule(r"(\.)([a-zA-Z_]\w*)").groups(&[T::Operator, T::NameAttribute]),
            rule(r"(void|bool|char|double|float|int|int8|int16|int32|int64|long|short|size_t|ssize_t|string|time_t|uchar|uint|uint8|uint16|uint32|uint64|ulong|unichar|ushort)\b").token(T::KeywordType),
            rule(r"(true|false|null)\b").token(T::NameBuiltin),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("root", &[
            include("whitespace"),
            rule("").push(&["statement"]),
        ]),
        ("statement", &[
            include("whitespace"),
            include("statements"),
            rule(r"[{}]").token(T::Punctuation),
            rule(r";").token(T::Punctuation).pop(1),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-fA-F0-9]{2,4}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
        ("if0", &[
            rule(r"^\s*#if.*?(?<!\\)\n").token(T::CommentPreproc).push(&[]),
            rule(r"^\s*#el(?:se|if).*\n").token(T::CommentPreproc).pop(1),
            rule(r"^\s*#endif.*?(?<!\\)\n").token(T::CommentPreproc).pop(1),
            rule(r".*?\n").token(T::Comment),
        ]),
        ("class", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameClass).pop(1),
        ]),
        ("namespace", &[
            rule(r"[a-zA-Z_][\w.]*").token(T::NameNamespace).pop(1),
        ]),
    ],
};
