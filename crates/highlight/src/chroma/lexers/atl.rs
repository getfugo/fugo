//! Chroma's `atl.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "atl",
    config: ConfigDef {
        name: "ATL",
        aliases: &["atl"],
        filenames: &["*.atl"],
        mime_types: &["text/x-atl"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(--.*?)(\n)").groups(&[T::CommentSingle, T::TextWhitespace]),
            rule(r"(and|distinct|endif|else|for|foreach|if|implies|in|let|not|or|self|super|then|thisModule|xor)\b").token(T::Keyword),
            rule(r"(OclUndefined|true|false|#\w+)\b").token(T::KeywordConstant),
            rule(r"(module|query|library|create|from|to|uses)\b").token(T::KeywordNamespace),
            rule(r"(do)(\s*)({)").groups(&[T::KeywordNamespace, T::TextWhitespace, T::Punctuation]),
            rule(r"(abstract|endpoint|entrypoint|lazy|unique)(\s+)").groups(&[T::KeywordDeclaration, T::TextWhitespace]),
            rule(r"(rule)(\s+)").groups(&[T::KeywordNamespace, T::TextWhitespace]),
            rule(r"(helper)(\s+)").groups(&[T::KeywordNamespace, T::TextWhitespace]),
            rule(r"(context)(\s+)").groups(&[T::KeywordNamespace, T::TextWhitespace]),
            rule(r"(def)(\s*)(:)(\s*)").groups(&[T::KeywordNamespace, T::TextWhitespace, T::Punctuation, T::TextWhitespace]),
            rule(r"(Bag|Boolean|Integer|OrderedSet|Real|Sequence|Set|String|Tuple)").token(T::KeywordType),
            rule(r"(\w+)(\s*)(<-|<:=)").groups(&[T::NameNamespace, T::TextWhitespace, T::Punctuation]),
            rule(r#"#""#).token(T::KeywordConstant).push(&["quotedenumliteral"]),
            rule(r#"""#).token(T::NameNamespace).push(&["quotedname"]),
            rule(r"[^\S\n]+").token(T::TextWhitespace),
            rule(r"'").token(T::LiteralString).push(&["string"]),
            rule(r"[0-9]*\.[0-9]+").token(T::LiteralNumberFloat),
            rule(r"0|[1-9][0-9]*").token(T::LiteralNumberInteger),
            rule(r"[*<>+=/-]").token(T::Operator),
            rule(r"([{}();:.,!|]|->)").token(T::Punctuation),
            rule(r"\n").token(T::TextWhitespace),
            rule(r"\w+").token(T::NameNamespace),
        ]),
        ("string", &[
            rule(r"[^\\']+").token(T::LiteralString),
            rule(r"\\\\").token(T::LiteralString),
            rule(r"\\'").token(T::LiteralString),
            rule(r"\\").token(T::LiteralString),
            rule(r"'").token(T::LiteralString).pop(1),
        ]),
        ("quotedname", &[
            rule(r#"[^\\"]+"#).token(T::NameNamespace),
            rule(r"\\\\").token(T::NameNamespace),
            rule(r#"\\""#).token(T::NameNamespace),
            rule(r"\\").token(T::NameNamespace),
            rule(r#"""#).token(T::NameNamespace).pop(1),
        ]),
        ("quotedenumliteral", &[
            rule(r#"[^\\"]+"#).token(T::KeywordConstant),
            rule(r"\\\\").token(T::KeywordConstant),
            rule(r#"\\""#).token(T::KeywordConstant),
            rule(r"\\").token(T::KeywordConstant),
            rule(r#"""#).token(T::KeywordConstant).pop(1),
        ]),
    ],
};
