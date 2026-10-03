//! Chroma's `c.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "c",
    config: ConfigDef {
        name: "C",
        aliases: &["c"],
        filenames: &["*.c", "*.h", "*.idc", "*.x[bp]m"],
        mime_types: &["text/x-chdr", "text/x-csrc", "image/x-xbitmap", "image/x-xpixmap"],
        ensure_nl: true,
        analyse: Some(AnalyseDef {
            first: true,
            regexes: &[
                (r"(?m)^\s*#include <", 0.1),
                (r"(?m)^\s*#ifn?def ", 0.1),
            ],
        }),
        ..ConfigDef::EMPTY
    },
    states: &[
        ("statement", &[
            include("whitespace"),
            include("statements"),
            rule(r"[{}]").token(T::Punctuation),
            rule(r";").token(T::Punctuation).pop(1),
        ]),
        ("function", &[
            include("whitespace"),
            include("statements"),
            rule(r";").token(T::Punctuation),
            rule(r"\{").token(T::Punctuation).push(&[]),
            rule(r"\}").token(T::Punctuation).pop(1),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\([\\abfnrtv"\']|x[a-fA-F0-9]{2,4}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{8}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r"\\\n").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
        ]),
        ("macro", &[
            rule(r"(include)(\s*(?:/[*].*?[*]/\s*)?)([^\n]+)").groups(&[T::CommentPreproc, T::Text, T::CommentPreprocFile]),
            rule(r"[^/\n]+").token(T::CommentPreproc),
            rule(r"/[*](.|\n)*?[*]/").token(T::CommentMultiline),
            rule(r"//.*?\n").token(T::CommentSingle).pop(1),
            rule(r"/").token(T::CommentPreproc),
            rule(r"(?<=\\)\n").token(T::CommentPreproc),
            rule(r"\n").token(T::CommentPreproc).pop(1),
        ]),
        ("if0", &[
            rule(r"^\s*#if.*?(?<!\\)\n").token(T::CommentPreproc).push(&[]),
            rule(r"^\s*#el(?:se|if).*\n").token(T::CommentPreproc).pop(1),
            rule(r"^\s*#endif.*?(?<!\\)\n").token(T::CommentPreproc).pop(1),
            rule(r".*?\n").token(T::Comment),
        ]),
        ("whitespace", &[
            rule(r"^#if\s+0").token(T::CommentPreproc).push(&["if0"]),
            rule(r"^#").token(T::CommentPreproc).push(&["macro"]),
            rule(r"^(\s*(?:/[*].*?[*]/\s*)?)(#if\s+0)").bygroups(&[E::UsingSelf("root"), E::Token(T::CommentPreproc)]).push(&["if0"]),
            rule(r"^(\s*(?:/[*].*?[*]/\s*)?)(#)").bygroups(&[E::UsingSelf("root"), E::Token(T::CommentPreproc)]).push(&["macro"]),
            rule(r"\n").token(T::Text),
            rule(r"\s+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"//(\n|[\w\W]*?[^\\]\n)").token(T::CommentSingle),
            rule(r"/(\\\n)?[*][\w\W]*?[*](\\\n)?/").token(T::CommentMultiline),
            rule(r"/(\\\n)?[*][\w\W]*").token(T::CommentMultiline),
        ]),
        ("statements", &[
            rule(r#"(L?)(")"#).groups(&[T::LiteralStringAffix, T::LiteralString]).push(&["string"]),
            rule(r"(L?)(')(\\.|\\[0-7]{1,3}|\\x[a-fA-F0-9]{1,2}|[^\\\'\n])(')").groups(&[T::LiteralStringAffix, T::LiteralStringChar, T::LiteralStringChar, T::LiteralStringChar]),
            rule(r"(\d+\.\d*|\.\d+|\d+)[eE][+-]?\d+[LlUu]*").token(T::LiteralNumberFloat),
            rule(r"(\d+\.\d*|\.\d+|\d+[fF])[fF]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+[LlUu]*").token(T::LiteralNumberHex),
            rule(r"0[0-7]+[LlUu]*").token(T::LiteralNumberOct),
            rule(r"\d+[LlUu]*").token(T::LiteralNumberInteger),
            rule(r"\*/").token(T::Error),
            rule(r"[~!%^&*+=|?:<>/-]").token(T::Operator),
            rule(r"[()\[\],.]").token(T::Punctuation),
            rule(r"(restricted|volatile|continue|register|default|typedef|struct|extern|switch|sizeof|static|return|union|while|const|break|goto|enum|else|case|auto|for|asm|if|do)\b").token(T::Keyword),
            rule(r"(bool|int|long|float|short|double|char((8|16|32)_t)?|unsigned|signed|void|u?int(_fast|_least|)(8|16|32|64)_t)\b|\b[a-z]\w*_t\b").token(T::KeywordType),
            rule(r"(typename|__inline|restrict|_inline|thread|inline|naked)\b").token(T::KeywordReserved),
            rule(r"(__m(128i|128d|128|64))\b").token(T::KeywordReserved),
            rule(r"__(forceinline|identifier|unaligned|declspec|fastcall|finally|stdcall|wchar_t|assume|except|int32|cdecl|int16|leave|based|raise|int64|noop|int8|w64|try|asm)\b").token(T::KeywordReserved),
            rule(r"(true|false|NULL)\b").token(T::NameBuiltin),
            rule(r"([a-zA-Z_]\w*)(\s*)(:)(?!:)").groups(&[T::NameLabel, T::Text, T::Punctuation]),
            rule(r"\b[A-Za-z_]\w*(?=\s*\()").token(T::NameFunction),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("root", &[
            include("whitespace"),
            rule(r"((?:[\w*\s])+?(?:\s|[*]))([a-zA-Z_]\w*)(\s*\([^;]*?\))([^;{]*)(\{)").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::UsingSelf("root"), E::UsingSelf("root"), E::Token(T::Punctuation)]).push(&["function"]),
            rule(r"((?:[\w*\s])+?(?:\s|[*]))([a-zA-Z_]\w*)(\s*\([^;]*?\))([^;]*)(;)").bygroups(&[E::UsingSelf("root"), E::Token(T::NameFunction), E::UsingSelf("root"), E::UsingSelf("root"), E::Token(T::Punctuation)]),
            rule("").push(&["statement"]),
        ]),
    ],
};
