//! Chroma's `zig.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "zig",
    config: ConfigDef {
        name: "Zig",
        aliases: &["zig"],
        filenames: &["*.zig", "*.zon"],
        mime_types: &["text/zig"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#"\\(x[a-fA-F0-9]{2}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{6}|[nr\\t\'"])"#).token(T::LiteralStringEscape),
            rule(r#"[^\\"\n]+"#).token(T::LiteralString),
            rule(r#"""#).token(T::LiteralString).pop(1),
        ]),
        ("root", &[
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"(unreachable|continue|errdefer|suspend|return|resume|cancel|break|catch|async|await|defer|asm|try)\b").token(T::Keyword),
            rule(r"(threadlocal|linksection|allowzero|stdcallcc|volatile|comptime|noalias|nakedcc|inline|export|packed|extern|align|const|pub|var)\b").token(T::KeywordReserved),
            rule(r"(struct|union|error|enum)\b").token(T::Keyword),
            rule(r"(while|for)\b").token(T::Keyword),
            rule(r"(comptime_float|comptime_int|c_longdouble|c_ulonglong|c_longlong|c_voidi8|noreturn|c_ushort|anyerror|promise|c_short|c_ulong|c_uint|c_long|isize|c_int|usize|void|f128|i128|type|bool|u128|u16|f64|f32|u64|i16|f16|i32|u32|i64|u8|i0|u0)\b").token(T::KeywordType),
            rule(r"(undefined|false|true|null)\b").token(T::KeywordConstant),
            rule(r"(switch|orelse|else|and|if|or)\b").token(T::Keyword),
            rule(r"(usingnamespace|test|fn)\b").token(T::Keyword),
            rule(r"0x[0-9a-fA-F]+\.[0-9a-fA-F]+([pP][\-+]?[0-9a-fA-F]+)?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+\.?[pP][\-+]?[0-9a-fA-F]+").token(T::LiteralNumberFloat),
            rule(r"[0-9]+\.[0-9]+([eE][-+]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"[0-9]+\.?[eE][-+]?[0-9]+").token(T::LiteralNumberFloat),
            rule(r"0b(?:_?[01])+").token(T::LiteralNumberBin),
            rule(r"0o(?:_?[0-7])+").token(T::LiteralNumberOct),
            rule(r"0x(?:_?[0-9a-fA-F])+").token(T::LiteralNumberHex),
            rule(r"(?:_?[0-9])+").token(T::LiteralNumberInteger),
            rule(r"\b[A-Za-z_]\w*(?=\s*\()").token(T::NameFunction),
            rule(r"@[a-zA-Z_]\w*").token(T::NameBuiltin),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule(r"\'\\\'\'").token(T::LiteralStringEscape),
            rule(r#"\'\\(|x[a-fA-F0-9]{2}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{6}|[nr\\t\'"])\'"#).token(T::LiteralStringEscape),
            rule(r"\'[^\\\']\'").token(T::LiteralString),
            rule(r"\\\\[^\n]*").token(T::LiteralStringHeredoc),
            rule(r"c\\\\[^\n]*").token(T::LiteralStringHeredoc),
            rule(r#"c?""#).token(T::LiteralString).push(&["string"]),
            rule(r"[+%=><|^!?/\-*&~:]").token(T::Operator),
            rule(r"[{}()\[\],.;]").token(T::Punctuation),
        ]),
    ],
};
