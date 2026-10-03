//! Chroma's `smali.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

// Generated from https://github.com/pygments/pygments/blob/15f222adefd2bf7835bfd74a12d720028ae68d29/pygments/lexers/dalvik.py.
#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "smali",
    config: ConfigDef {
        name: "Smali",
        aliases: &["smali"],
        filenames: &["*.smali"],
        mime_types: &["text/smali"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            include("comment"),
            include("label"),
            include("field"),
            include("method"),
            include("class"),
            include("directive"),
            include("access-modifier"),
            include("instruction"),
            include("literal"),
            include("punctuation"),
            include("type"),
            include("whitespace"),
        ]),
        ("directive", &[
            rule(r"^([ \t]*)(\.(?:class|super|implements|field|subannotation|annotation|enum|method|registers|locals|array-data|packed-switch|sparse-switch|catchall|catch|line|parameter|local|prologue|epilogue|source))").groups(&[T::TextWhitespace, T::Keyword]),
            rule(r"^([ \t]*)(\.end)( )(field|subannotation|annotation|method|array-data|packed-switch|sparse-switch|parameter|local)").groups(&[T::TextWhitespace, T::Keyword, T::TextWhitespace, T::Keyword]),
            rule(r"^([ \t]*)(\.restart)( )(local)").groups(&[T::TextWhitespace, T::Keyword, T::TextWhitespace, T::Keyword]),
        ]),
        ("access-modifier", &[
            rule(r"(public|private|protected|static|final|synchronized|bridge|varargs|native|abstract|strictfp|synthetic|constructor|declared-synchronized|interface|enum|annotation|volatile|transient)").token(T::Keyword),
        ]),
        ("whitespace", &[
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\s+").token(T::TextWhitespace),
        ]),
        ("instruction", &[
            rule(r"\b[vp]\d+\b").token(T::NameBuiltin),
            rule(r"(\b[a-z][A-Za-z0-9/-]+)(\s+)").groups(&[T::Text, T::TextWhitespace]),
        ]),
        ("literal", &[
            rule(r#"".*""#).token(T::LiteralString),
            rule(r"0x[0-9A-Fa-f]+t?").token(T::LiteralNumberHex),
            rule(r"[0-9]*\.[0-9]+([eE][0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"[0-9]+L?").token(T::LiteralNumberInteger),
        ]),
        ("field", &[
            rule(r"(\$?\b)([\w$]*)(:)").groups(&[T::Punctuation, T::NameVariable, T::Punctuation]),
        ]),
        ("method", &[
            rule(r"<(?:cl)?init>").token(T::NameFunction),
            rule(r"(\$?\b)([\w$]*)(\()").groups(&[T::Punctuation, T::NameFunction, T::Punctuation]),
        ]),
        ("label", &[
            rule(r":\w+").token(T::NameLabel),
        ]),
        ("class", &[
            rule(r"(L)((?:[\w$]+/)*)([\w$]+)(;)").groups(&[T::KeywordType, T::Text, T::NameClass, T::Text]),
        ]),
        ("punctuation", &[
            rule(r"->").token(T::Punctuation),
            rule(r"[{},():=.-]").token(T::Punctuation),
        ]),
        ("type", &[
            rule(r"[ZBSCIJFDV\[]+").token(T::KeywordType),
        ]),
        ("comment", &[
            rule(r"#.*?\n").token(T::Comment),
        ]),
    ],
};
