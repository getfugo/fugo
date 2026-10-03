//! Chroma's `nasm.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "nasm",
    config: ConfigDef {
        name: "NASM",
        aliases: &["nasm"],
        filenames: &["*.asm", "*.ASM", "*.nasm"],
        mime_types: &["text/x-nasm"],
        case_insensitive: true,
        priority: 1.0, // TASM uses the same file endings, but TASM is not as common as NASM, so we prioritize NASM higher by default.
        ..ConfigDef::EMPTY
    },
    states: &[
        ("punctuation", &[
            rule(r"[,():\[\]]+").token(T::Punctuation),
            rule(r"[&|^<>+*/%~-]+").token(T::Operator),
            rule(r"[$]+").token(T::KeywordConstant),
            rule(r"seg|wrt|strict").token(T::OperatorWord),
            rule(r"byte|[dq]?word").token(T::KeywordType),
        ]),
        ("root", &[
            rule(r"^\s*%").token(T::CommentPreproc).push(&["preproc"]),
            include("whitespace"),
            rule(r"[a-z$._?][\w$.?#@~]*:").token(T::NameLabel),
            rule(r"([a-z$._?][\w$.?#@~]*)(\s+)(equ)").groups(&[T::NameConstant, T::KeywordDeclaration, T::KeywordDeclaration]).push(&["instruction-args"]),
            rule(r"BITS|USE16|USE32|SECTION|SEGMENT|ABSOLUTE|EXTERN|GLOBAL|ORG|ALIGN|STRUC|ENDSTRUC|COMMON|CPU|GROUP|UPPERCASE|IMPORT|EXPORT|LIBRARY|MODULE").token(T::Keyword).push(&["instruction-args"]),
            rule(r"(?:res|d)[bwdqt]|times").token(T::KeywordDeclaration).push(&["instruction-args"]),
            rule(r"[a-z$._?][\w$.?#@~]*").token(T::NameFunction).push(&["instruction-args"]),
            rule(r"[\r\n]+").token(T::Text),
        ]),
        ("instruction-args", &[
            rule(r#""(\\"|[^"\n])*"|'(\\'|[^'\n])*'|`(\\`|[^`\n])*`"#).token(T::LiteralString),
            rule(r"(?:0x[0-9a-f]+|$0[0-9a-f]*|[0-9]+[0-9a-f]*h)").token(T::LiteralNumberHex),
            rule(r"[0-7]+q").token(T::LiteralNumberOct),
            rule(r"[01]+b").token(T::LiteralNumberBin),
            rule(r"[0-9]+\.e?[0-9]+").token(T::LiteralNumberFloat),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            include("punctuation"),
            rule(r"r[0-9][0-5]?[bwd]|[a-d][lh]|[er]?[a-d]x|[er]?[sb]p|[er]?[sd]i|[c-gs]s|st[0-7]|mm[0-7]|cr[0-4]|dr[0-367]|tr[3-7]").token(T::NameBuiltin),
            rule(r"[a-z$._?][\w$.?#@~]*").token(T::NameVariable),
            rule(r"[\r\n]+").token(T::Text).pop(1),
            include("whitespace"),
        ]),
        ("preproc", &[
            rule(r"[^;\n]+").token(T::CommentPreproc),
            rule(r";.*?\n").token(T::CommentSingle).pop(1),
            rule(r"\n").token(T::CommentPreproc).pop(1),
        ]),
        ("whitespace", &[
            rule(r"\n").token(T::Text),
            rule(r"[ \t]+").token(T::Text),
            rule(r";.*").token(T::CommentSingle),
        ]),
    ],
};
