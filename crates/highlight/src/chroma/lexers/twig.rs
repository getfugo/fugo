//! Chroma's `twig.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "twig",
    config: ConfigDef {
        name: "Twig",
        aliases: &["twig"],
        filenames: &["*.twig"],
        mime_types: &["application/x-twig"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("var", &[
            rule(r"\s+").token(T::Text),
            rule(r"(-?)(\}\})").groups(&[T::Text, T::CommentPreproc]).pop(1),
            include("varnames"),
        ]),
        ("tag", &[
            rule(r"\s+").token(T::Text),
            rule(r"(-?)(%\})").groups(&[T::Text, T::CommentPreproc]).pop(1),
            include("varnames"),
            rule(r".").token(T::Punctuation),
        ]),
        ("root", &[
            rule(r"[^{]+").token(T::Other),
            rule(r"\{\{").token(T::CommentPreproc).push(&["var"]),
            rule(r"\{\#.*?\#\}").token(T::Comment),
            rule(r"(\{%)(-?\s*)(raw)(\s*-?)(%\})(.*?)(\{%)(-?\s*)(endraw)(\s*-?)(%\})").groups(&[T::CommentPreproc, T::Text, T::Keyword, T::Text, T::CommentPreproc, T::Other, T::CommentPreproc, T::Text, T::Keyword, T::Text, T::CommentPreproc]),
            rule(r"(\{%)(-?\s*)(verbatim)(\s*-?)(%\})(.*?)(\{%)(-?\s*)(endverbatim)(\s*-?)(%\})").groups(&[T::CommentPreproc, T::Text, T::Keyword, T::Text, T::CommentPreproc, T::Other, T::CommentPreproc, T::Text, T::Keyword, T::Text, T::CommentPreproc]),
            rule(r"(\{%)(-?\s*)(filter)(\s+)((?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w-]|[^\x00-\x7f])*)").groups(&[T::CommentPreproc, T::Text, T::Keyword, T::Text, T::NameFunction]).push(&["tag"]),
            rule(r"(\{%)(-?\s*)([a-zA-Z_]\w*)").groups(&[T::CommentPreproc, T::Text, T::Keyword]).push(&["tag"]),
            rule(r"\{").token(T::Other),
        ]),
        ("varnames", &[
            rule(r"(\|)(\s*)((?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w-]|[^\x00-\x7f])*)").groups(&[T::Operator, T::Text, T::NameFunction]),
            rule(r"(is)(\s+)(not)?(\s*)((?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w-]|[^\x00-\x7f])*)").groups(&[T::Keyword, T::Text, T::Keyword, T::Text, T::NameFunction]),
            rule(r"(?i)(true|false|none|null)\b").token(T::KeywordPseudo),
            rule(r"(in|not|and|b-and|or|b-or|b-xor|isif|elseif|else|importconstant|defined|divisibleby|empty|even|iterable|odd|sameasmatches|starts\s+with|ends\s+with)\b").token(T::Keyword),
            rule(r"(loop|block|parent)\b").token(T::NameBuiltin),
            rule(r"(?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w-]|[^\x00-\x7f])*").token(T::NameVariable),
            rule(r"\.(?:[\\_a-z]|[^\x00-\x7f])(?:[\\\w-]|[^\x00-\x7f])*").token(T::NameVariable),
            rule(r"\.[0-9]+").token(T::LiteralNumber),
            rule(r#":?"(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r":?'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"([{}()\[\]+\-*/,:~%]|\.\.|\?|:|\*\*|\/\/|!=|[><=]=?)").token(T::Operator),
            rule(r"[0-9](\.[0-9]*)?(eE[+-][0-9])?[flFLdD]?|0[xX][0-9a-fA-F]+[Ll]?").token(T::LiteralNumber),
        ]),
    ],
};
