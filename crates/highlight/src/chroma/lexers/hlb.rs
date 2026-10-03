//! Chroma's `hlb.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "hlb",
    config: ConfigDef {
        name: "HLB",
        aliases: &["hlb"],
        filenames: &["*.hlb"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(#.*)").token(T::CommentSingle),
            rule(r"((\b(0(b|B|o|O|x|X)[a-fA-F0-9]+)\b)|(\b(0|[1-9][0-9]*)\b))").token(T::LiteralNumber),
            rule(r"((\b(true|false)\b))").token(T::NameBuiltin),
            rule(r"(\bstring\b|\bint\b|\bbool\b|\bfs\b|\boption\b)").token(T::KeywordType),
            rule(r"(\b[a-zA-Z_][a-zA-Z0-9]*\b)(\()").groups(&[T::NameFunction, T::Punctuation]).push(&["params"]),
            rule(r"(\{)").token(T::Punctuation).push(&["block"]),
            rule(r"(\n|\r|\r\n)").token(T::Text),
            rule(r".").token(T::Text),
        ]),
        ("string", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r#"\\""#).token(T::LiteralString),
            rule(r#"[^\\"]+"#).token(T::LiteralString),
        ]),
        ("block", &[
            rule(r"(\})").token(T::Punctuation).pop(1),
            rule(r"(#.*)").token(T::CommentSingle),
            rule(r"((\b(0(b|B|o|O|x|X)[a-fA-F0-9]+)\b)|(\b(0|[1-9][0-9]*)\b))").token(T::LiteralNumber),
            rule(r"((\b(true|false)\b))").token(T::KeywordConstant),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"(with)").token(T::KeywordReserved),
            rule(r"(as)([\t ]+)(\b[a-zA-Z_][a-zA-Z0-9]*\b)").groups(&[T::KeywordReserved, T::Text, T::NameFunction]),
            rule(r"(\bstring\b|\bint\b|\bbool\b|\bfs\b|\boption\b)([\t ]+)(\{)").groups(&[T::KeywordType, T::Text, T::Punctuation]).push(&["block"]),
            rule(r"(?!\b(?:scratch|image|resolve|http|checksum|chmod|filename|git|keepGitDir|local|includePatterns|excludePatterns|followPaths|generate|frontendInput|shell|run|readonlyRootfs|env|dir|user|network|security|host|ssh|secret|mount|target|localPath|uid|gid|mode|readonly|tmpfs|sourcePath|cache|mkdir|createParents|chown|createdTime|mkfile|rm|allowNotFound|allowWildcards|copy|followSymlinks|contentsOnly|unpack|createDestPath)\b)(\b[a-zA-Z_][a-zA-Z0-9]*\b)").groups(&[T::NameOther]),
            rule(r"(\n|\r|\r\n)").token(T::Text),
            rule(r".").token(T::Text),
        ]),
        ("params", &[
            rule(r"(\))").groups(&[T::Punctuation]).pop(1),
            rule(r"(variadic)").groups(&[T::Keyword]),
            rule(r"(\bstring\b|\bint\b|\bbool\b|\bfs\b|\boption\b)").groups(&[T::KeywordType]),
            rule(r"(\b[a-zA-Z_][a-zA-Z0-9]*\b)").groups(&[T::NameOther]),
            rule(r"(\n|\r|\r\n)").token(T::Text),
            rule(r".").token(T::Text),
        ]),
    ],
};
