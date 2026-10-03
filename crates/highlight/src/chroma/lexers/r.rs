//! Chroma's `r.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "r",
    config: ConfigDef {
        name: "R",
        aliases: &["splus", "s", "r"],
        filenames: &["*.S", "*.R", "*.r", ".Rhistory", ".Rprofile", ".Renviron"],
        mime_types: &[
            "text/S-plus",
            "text/S",
            "text/x-r-source",
            "text/x-r",
            "text/x-R",
            "text/x-r-history",
            "text/x-r-profile",
        ],
        priority: 0.1, // higher priority than Rebol
        ..ConfigDef::EMPTY
    },
    states: &[
        ("numbers", &[
            rule(r"0[xX][a-fA-F0-9]+([pP][0-9]+)?[Li]?").token(T::LiteralNumberHex),
            rule(r"[+-]?([0-9]+(\.[0-9]+)?|\.[0-9]+|\.)([eE][+-]?[0-9]+)?[Li]?").token(T::LiteralNumber),
        ]),
        ("operators", &[
            rule(r"<<?-|->>?|-|==|<=|>=|<|>|&&?|!=|\|\|?|\?").token(T::Operator),
            rule(r"\*|\+|\^|/|!|%[^%]*%|=|~|\$|@|:{1,3}").token(T::Operator),
        ]),
        ("root", &[
            include("keywords"),
            rule(r"((?:`[^`\\]*(?:\\.[^`\\]*)*`)|(?:(?:[a-zA-z]|[_.][^0-9])[\w_.]*))\s*(?=\()").token(T::NameFunction),
            include("statements"),
            rule(r"\{|\}").token(T::Punctuation),
            rule(r".").token(T::Text),
        ]),
        ("valid_name", &[
            rule(r"(?:`[^`\\]*(?:\\.[^`\\]*)*`)|(?:(?:[a-zA-z]|[_.][^0-9])[\w_.]*)").token(T::Name),
        ]),
        ("keywords", &[
            rule(r"(if|else|for|while|repeat|in|next|break|return|switch|function)(?![\w.])").token(T::KeywordReserved),
        ]),
        ("builtin_symbols", &[
            rule(r"(NULL|NA(_(integer|real|complex|character)_)?|letters|LETTERS|Inf|TRUE|FALSE|NaN|pi|\.\.(\.|[0-9]+))(?![\w.])").token(T::KeywordConstant),
            rule(r"(T|F)\b").token(T::NameBuiltinPseudo),
        ]),
        ("string_squote", &[
            rule(r"([^\'\\]|\\.)*\'").token(T::LiteralString).pop(1),
        ]),
        ("comments", &[
            rule(r"#.*$").token(T::CommentSingle),
        ]),
        ("punctuation", &[
            rule(r"\[{1,2}|\]{1,2}|\(|\)|;|,").token(T::Punctuation),
        ]),
        ("statements", &[
            include("comments"),
            rule(r"\s+").token(T::Text),
            rule(r"\'").token(T::LiteralString).push(&["string_squote"]),
            rule(r#"\""#).token(T::LiteralString).push(&["string_dquote"]),
            include("builtin_symbols"),
            include("valid_name"),
            include("numbers"),
            include("punctuation"),
            include("operators"),
        ]),
        ("string_dquote", &[
            rule(r#"([^"\\]|\\.)*""#).token(T::LiteralString).pop(1),
        ]),
    ],
};
