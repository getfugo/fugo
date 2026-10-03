//! Chroma's `stas.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "stas",
    config: ConfigDef {
        name: "stas",
        filenames: &["*.stas"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string-double-quoted", &[
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r#"[^\\"]+"#).token(T::LiteralString),
            rule(r#"""#).token(T::LiteralString).pop(1),
        ]),
        ("string-single-quoted", &[
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r"[^\\']+").token(T::LiteralString),
            rule(r"'").token(T::LiteralString).pop(1),
        ]),
        ("string-char-literal", &[
            rule(r"\\.").token(T::LiteralStringEscape),
            rule(r"[^\\`]+").token(T::LiteralString),
            rule(r"`").token(T::LiteralStringChar).pop(1),
        ]),
        ("root", &[
            rule(r"(\n|\s)+").token(T::Text),
            rule(r"(?<!\S)(fn|argc|argv|swap|dup|over|over2|rot|rot4|drop|w8|w16|w32|w64|r8|r16|r32|r64|syscall0|syscall1|syscall2|syscall3|syscall4|syscall5|syscall6|_breakpoint|assert|const|auto|reserve|pop|include|addr|if|else|elif|while|break|continue|ret)(?!\S)").token(T::Keyword),
            rule(r"(?<!\S)(\+|\-|\*|\/|\%|\%\%|\+\+|\-\-|>>|<<)(?!\S)").token(T::Operator),
            rule(r"(?<!\S)(\=|\!\=|>|<|>\=|<\=|>s|<s|>\=s|<\=s)(?!\S)").token(T::Operator),
            rule(r"(?<!\S)(\&|\||\^|\~|\!|-\>)(?!\S)").token(T::Operator),
            rule(r"(?<!\S)\-?(\d+)(?!\S)").token(T::LiteralNumber),
            rule(r"(?<!\S);.*(\S|\n)").token(T::Comment),
            rule(r"'").token(T::LiteralString).push(&["string-single-quoted"]),
            rule(r#"""#).token(T::LiteralString).push(&["string-double-quoted"]),
            rule(r"`").token(T::LiteralStringChar).push(&["string-char-literal"]),
            rule(r"(?<!\S)[{}](?!\S)").token(T::Punctuation),
            rule(r"(?<!\S)[^\s]+(?!\S)").token(T::Name),
        ]),
    ],
};
