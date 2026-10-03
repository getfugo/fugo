//! Chroma's `blitzbasic.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "blitzbasic",
    config: ConfigDef {
        name: "BlitzBasic",
        aliases: &["blitzbasic", "b3d", "bplus"],
        filenames: &["*.bb", "*.decls"],
        mime_types: &["text/x-bb"],
        case_insensitive: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#""""#).token(T::LiteralStringDouble),
            rule(r#""C?"#).token(T::LiteralStringDouble).pop(1),
            rule(r#"[^"]+"#).token(T::LiteralStringDouble),
        ]),
        ("root", &[
            rule(r"[ \t]+").token(T::Text),
            rule(r";.*?\n").token(T::CommentSingle),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"[0-9]+\.[0-9]*(?!\.)").token(T::LiteralNumberFloat),
            rule(r"\.[0-9]+(?!\.)").token(T::LiteralNumberFloat),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"\$[0-9a-f]+").token(T::LiteralNumberHex),
            rule(r"\%[10]+").token(T::LiteralNumberBin),
            rule(r"\b(Before|Handle|After|First|Float|Last|Sgn|Abs|Not|And|Int|Mod|Str|Sar|Shr|Shl|Or)\b").token(T::Operator),
            rule(r"([+\-*/~=<>^])").token(T::Operator),
            rule(r"[(),:\[\]\\]").token(T::Punctuation),
            rule(r"\.([ \t]*)([a-z]\w*)").token(T::NameLabel),
            rule(r"\b(New)\b([ \t]+)([a-z]\w*)").groups(&[T::KeywordReserved, T::Text, T::NameClass]),
            rule(r"\b(Gosub|Goto)\b([ \t]+)([a-z]\w*)").groups(&[T::KeywordReserved, T::Text, T::NameLabel]),
            rule(r"\b(Object)\b([ \t]*)([.])([ \t]*)([a-z]\w*)\b").groups(&[T::Operator, T::Text, T::Punctuation, T::Text, T::NameClass]),
            rule(r"\b([a-z]\w*)(?:([ \t]*)(@{1,2}|[#$%])|([ \t]*)([.])([ \t]*)(?:([a-z]\w*)))?\b([ \t]*)(\()").groups(&[T::NameFunction, T::Text, T::KeywordType, T::Text, T::Punctuation, T::Text, T::NameClass, T::Text, T::Punctuation]),
            rule(r"\b(Function)\b([ \t]+)([a-z]\w*)(?:([ \t]*)(@{1,2}|[#$%])|([ \t]*)([.])([ \t]*)(?:([a-z]\w*)))?").groups(&[T::KeywordReserved, T::Text, T::NameFunction, T::Text, T::KeywordType, T::Text, T::Punctuation, T::Text, T::NameClass]),
            rule(r"\b(Type)([ \t]+)([a-z]\w*)").groups(&[T::KeywordReserved, T::Text, T::NameClass]),
            rule(r"\b(Pi|True|False|Null)\b").token(T::KeywordConstant),
            rule(r"\b(Local|Global|Const|Field|Dim)\b").token(T::KeywordDeclaration),
            rule(r"\b(Function|Restore|Default|Forever|Include|Return|Repeat|ElseIf|Delete|Insert|Select|EndIf|Until|While|Gosub|Type|Goto|Else|Data|Next|Step|Each|Case|Wend|Exit|Read|Then|For|New|Asc|Len|Chr|End|To|If)\b").token(T::KeywordReserved),
            rule(r"([a-z]\w*)(?:([ \t]*)(@{1,2}|[#$%])|([ \t]*)([.])([ \t]*)(?:([a-z]\w*)))?").groups(&[T::NameVariable, T::Text, T::KeywordType, T::Text, T::Punctuation, T::Text, T::NameClass]),
        ]),
    ],
};
