//! Chroma's `j.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "j",
    config: ConfigDef {
        name: "J",
        aliases: &["j"],
        filenames: &["*.ijs"],
        mime_types: &["text/x-j"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("singlequote", &[
            rule(r"[^']").token(T::LiteralString),
            rule(r"''").token(T::LiteralString),
            rule(r"'").token(T::LiteralString).pop(1),
        ]),
        ("root", &[
            rule(r"#!.*$").token(T::CommentPreproc),
            rule(r"NB\..*").token(T::CommentSingle),
            rule(r"\n+\s*Note").token(T::CommentMultiline).push(&["comment"]),
            rule(r"\s*Note.*").token(T::CommentSingle),
            rule(r"\s+").token(T::Text),
            rule(r"'").token(T::LiteralString).push(&["singlequote"]),
            rule(r"0\s+:\s*0|noun\s+define\s*$").token(T::NameEntity).push(&["nounDefinition"]),
            rule(r"(([1-4]|13)\s+:\s*0|(adverb|conjunction|dyad|monad|verb)\s+define)\b").token(T::NameFunction).push(&["explicitDefinition"]),
            rule(r"(label_|goto_|for_)\b[a-zA-Z]\w*\.").token(T::NameLabel),
            rule(r"(continue|select|return|assert|catchd|catcht|elseif|whilst|break|catch|fcase|while|throw|else|case|end|try|for|do|if)\.").token(T::NameLabel),
            rule(r"\b[a-zA-Z]\w*").token(T::NameVariable),
            rule(r"(timespacex|fixdotdot|nameclass|namelist|file2url|tmoutput|ucpcount|boxxopen|smoutput|JVERSION|datatype|toupper|tolower|alpha17|alpha27|getargs|evtloop|boxopen|fliprgb|inverse|scriptd|iospath|cutopen|isatty|toCRLF|toHOST|isutf8|getenv|stdout|script|usleep|sminfo|expand|stderr|clear|fetch|every|erase|empty|Debug|EMPTY|split|names|timex|cutLF|stdin|apply|items|table|exit|Note|list|take|leaf|type|bind|drop|rows|each|echo|sign|CRLF|utf8|sort|pick|ARGV|uucp|ucp|DEL|inv|hfd|dfh|def|LF2|EAV|toJ|TAB|nl|FF|LF|bx|nc|CR|on)").token(T::NameFunction),
            rule(r"=[.:]").token(T::Operator),
            rule(r#"[-=+*#$%@!~`^&";:.,<>{}\[\]\\|/]"#).token(T::Operator),
            rule(r"[abCdDeEfHiIjLMoprtT]\.").token(T::KeywordReserved),
            rule(r"[aDiLpqsStux]\:").token(T::KeywordReserved),
            rule(r"(_[0-9])\:").token(T::KeywordConstant),
            rule(r"\(").token(T::Punctuation).push(&["parentheses"]),
            include("numbers"),
        ]),
        ("comment", &[
            rule(r"[^)]").token(T::CommentMultiline),
            rule(r"^\)").token(T::CommentMultiline).pop(1),
            rule(r"[)]").token(T::CommentMultiline),
        ]),
        ("explicitDefinition", &[
            rule(r"\b[nmuvxy]\b").token(T::NameDecorator),
            include("root"),
            rule(r"[^)]").token(T::Name),
            rule(r"^\)").token(T::NameLabel).pop(1),
            rule(r"[)]").token(T::Name),
        ]),
        ("numbers", &[
            rule(r"\b_{1,2}\b").token(T::LiteralNumber),
            rule(r"_?\d+(\.\d+)?(\s*[ejr]\s*)_?\d+(\.?=\d+)?").token(T::LiteralNumber),
            rule(r"_?\d+\.(?=\d+)").token(T::LiteralNumberFloat),
            rule(r"_?\d+x").token(T::LiteralNumberIntegerLong),
            rule(r"_?\d+").token(T::LiteralNumberInteger),
        ]),
        ("nounDefinition", &[
            rule(r"[^)]").token(T::LiteralString),
            rule(r"^\)").token(T::NameLabel).pop(1),
            rule(r"[)]").token(T::LiteralString),
        ]),
        ("parentheses", &[
            rule(r"\)").token(T::Punctuation).pop(1),
            include("explicitDefinition"),
            include("root"),
        ]),
    ],
};
