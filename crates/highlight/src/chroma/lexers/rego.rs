//! Chroma's `rego.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "rego",
    config: ConfigDef {
        name: "Rego",
        aliases: &["rego"],
        filenames: &["*.rego"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(package|import|as|not|with|default|else|some|in|if|contains)\b").token(T::KeywordDeclaration),
            // importing keywords should then show up as keywords
            rule(r"(import)( future.keywords.)(\w+)").groups(&[T::KeywordDeclaration, T::Text, T::KeywordDeclaration]),
            rule(r"#[^\r\n]*").token(T::Comment),
            rule(r"(FIXME|TODO|XXX)\b( .*)$").groups(&[T::Error, T::CommentSpecial]),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"\d+i").token(T::LiteralNumber),
            rule(r"\d+\.\d*([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\.\d+([Ee][-+]\d+)?i").token(T::LiteralNumber),
            rule(r"\d+[Ee][-+]\d+i").token(T::LiteralNumber),
            rule(r"\d+(\.\d+[eE][+\-]?\d+|\.\d*|[eE][+\-]?\d+)").token(T::LiteralNumberFloat),
            rule(r"\.\d+([eE][+\-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"(0|[1-9][0-9]*)").token(T::LiteralNumberInteger),
            rule(r#"""".*?""""#).token(T::LiteralStringDouble),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"\$/((?!/\$).)*/\$").token(T::LiteralString),
            rule(r#"/(\\\\|\\"|[^/])*/"#).token(T::LiteralString),
            rule(r"^(\w+)").token(T::Name),
            rule(r"[a-z_-][\w-]*(?=\()").token(T::NameFunction),
            rule(r"[\r\n\s]+").token(T::TextWhitespace),
            rule(r"(package|import)(\s+)").groups(&[T::KeywordDeclaration, T::Text]),
            rule(r"[=<>!+-/*&|]").token(T::Operator),
            rule(r":=").token(T::Operator),
            rule(r"[[\]{}():;]+").token(T::Punctuation),
            rule(r"[$a-zA-Z_]\w*").token(T::NameOther),
        ]),
    ],
};
