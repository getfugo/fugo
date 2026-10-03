//! Chroma's `holyc.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "holyc",
    config: ConfigDef {
        name: "HolyC",
        aliases: &["holyc"],
        filenames: &["*.HC", "*.hc", "*.HH", "*.hh", "*.hc.z", "*.HC.Z"],
        mime_types: &["text/x-chdr", "text/x-csrc", "image/x-xbitmap", "image/x-xpixmap"],
        ensure_nl: true,
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
            rule(r"(break|case|continue|default|do|else|for|goto|if|return|switch|while|throw|try|catch|extern|MOV|CALL|PUSH|LEAVE|RET|SUB|SHR|ADD|RETF|CMP|JNE|BTS|INT|XOR|JC|JZ|LOOP|POP|TEST|SHL|ADC|SBB|JMP|INC)\b").token(T::Keyword),
            rule(r"(U0|I8|U8|I16|U16|I32|U32|I64|U64|F64|Bool|class|union|DU8|DU16|DU32|DU64|RAX|RCX|RDX|RBX|RSP|RBP|RSI|RDI|EAX|ECX|EDX|EBX|ESP|EBP|ESI|EDI|AX|CX|DX|BX|SP|BP|SI|DI|SS|CS|DS|ES|FS|GS|CH|asm|const|extern|register|restrict|static|volatile|inline|_extern|_import|IMPORT|public)\b").token(T::KeywordType),
            rule(r"__()\b").token(T::KeywordReserved),
            rule(r"(NULL|TRUE|FALSE|ON|OFF)\b").token(T::NameBuiltin),
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
