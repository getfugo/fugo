//! Chroma's `tasm.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "tasm",
    config: ConfigDef {
        name: "TASM",
        aliases: &["tasm"],
        filenames: &["*.asm", "*.ASM", "*.tasm"],
        mime_types: &["text/x-tasm"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("preproc", &[
            rule(r"[^;\n]+").token(T::CommentPreproc),
            rule(r";.*?\n").token(T::CommentSingle).pop(1),
            rule(r"\n").token(T::CommentPreproc).pop(1),
        ]),
        ("whitespace", &[
            rule(r"[\n\r]").token(T::Text),
            rule(r"\\[\n\r]").token(T::Text),
            rule(r"[ \t]+").token(T::Text),
            rule(r";.*").token(T::CommentSingle),
        ]),
        ("punctuation", &[
            rule(r"[,():\[\]]+").token(T::Punctuation),
            rule(r"[&|^<>+*=/%~-]+").token(T::Operator),
            rule(r"[$]+").token(T::KeywordConstant),
            rule(r"seg|wrt|strict").token(T::OperatorWord),
            rule(r"byte|[dq]?word").token(T::KeywordType),
        ]),
        ("root", &[
            rule(r"^\s*%").token(T::CommentPreproc).push(&["preproc"]),
            include("whitespace"),
            rule(r"[@a-z$._?][\w$.?#@~]*:").token(T::NameLabel),
            rule(r"BITS|USE16|USE32|SECTION|SEGMENT|ABSOLUTE|EXTERN|GLOBAL|ORG|ALIGN|STRUC|ENDSTRUC|ENDS|COMMON|CPU|GROUP|UPPERCASE|INCLUDE|EXPORT|LIBRARY|MODULE|PROC|ENDP|USES|ARG|DATASEG|UDATASEG|END|IDEAL|P386|MODEL|ASSUME|CODESEG|SIZE").token(T::Keyword).push(&["instruction-args"]),
            rule(r"([@a-z$._?][\w$.?#@~]*)(\s+)(db|dd|dw|T[A-Z][a-z]+)").groups(&[T::NameConstant, T::KeywordDeclaration, T::KeywordDeclaration]).push(&["instruction-args"]),
            rule(r"(?:res|d)[bwdqt]|times").token(T::KeywordDeclaration).push(&["instruction-args"]),
            rule(r"[@a-z$._?][\w$.?#@~]*").token(T::NameFunction).push(&["instruction-args"]),
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
            rule(r"[@a-z$._?][\w$.?#@~]*").token(T::NameVariable),
            rule(r"(\\\s*)(;.*)([\r\n])").groups(&[T::Text, T::CommentSingle, T::Text]),
            rule(r"[\r\n]+").token(T::Text).pop(1),
            include("whitespace"),
        ]),
    ],
};
