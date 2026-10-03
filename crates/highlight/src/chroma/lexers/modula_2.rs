//! Chroma's `modula-2.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "modula-2",
    config: ConfigDef {
        name: "Modula-2",
        aliases: &["modula2", "m2"],
        filenames: &["*.def", "*.mod"],
        mime_types: &["text/x-modula2"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("dialecttags", &[
            rule(r"\(\*!m2pim\*\)").token(T::CommentSpecial),
            rule(r"\(\*!m2iso\*\)").token(T::CommentSpecial),
            rule(r"\(\*!m2r10\*\)").token(T::CommentSpecial),
            rule(r"\(\*!objm2\*\)").token(T::CommentSpecial),
            rule(r"\(\*!m2iso\+aglet\*\)").token(T::CommentSpecial),
            rule(r"\(\*!m2pim\+gm2\*\)").token(T::CommentSpecial),
            rule(r"\(\*!m2iso\+p1\*\)").token(T::CommentSpecial),
            rule(r"\(\*!m2iso\+xds\*\)").token(T::CommentSpecial),
        ]),
        ("unigraph_operators", &[
            rule(r"[+-]").token(T::Operator),
            rule(r"[*/]").token(T::Operator),
            rule(r"\\").token(T::Operator),
            rule(r"[=#<>]").token(T::Operator),
            rule(r"\^").token(T::Operator),
            rule(r"@").token(T::Operator),
            rule(r"&").token(T::Operator),
            rule(r"~").token(T::Operator),
            rule(r"`").token(T::Operator),
        ]),
        ("string_literals", &[
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralString),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
        ]),
        ("identifiers", &[
            rule(r"([a-zA-Z_$][\w$]*)").token(T::Name),
        ]),
        ("pragmas", &[
            rule(r"<\*.*?\*>").token(T::CommentPreproc),
            rule(r"\(\*\$.*?\*\)").token(T::CommentPreproc),
        ]),
        ("comments", &[
            rule(r"^//.*?\n").token(T::CommentSingle),
            rule(r"\(\*([^$].*?)\*\)").token(T::CommentMultiline),
            rule(r"/\*(.*?)\*/").token(T::CommentMultiline),
        ]),
        ("whitespace", &[
            rule(r"\n+").token(T::Text),
            rule(r"\s+").token(T::Text),
        ]),
        ("suffixed_number_literals", &[
            rule(r"[0-7]+B").token(T::LiteralNumberOct),
            rule(r"[0-7]+C").token(T::LiteralNumberOct),
            rule(r"[0-9A-F]+H").token(T::LiteralNumberHex),
        ]),
        ("plain_number_literals", &[
            rule(r"[0-9]+(\'[0-9]+)*\.[0-9]+(\'[0-9]+)*[eE][+-]?[0-9]+(\'[0-9]+)*").token(T::LiteralNumberFloat),
            rule(r"[0-9]+(\'[0-9]+)*\.[0-9]+(\'[0-9]+)*").token(T::LiteralNumberFloat),
            rule(r"[0-9]+(\'[0-9]+)*").token(T::LiteralNumberInteger),
        ]),
        ("digraph_punctuation", &[
            rule(r"\.\.").token(T::Punctuation),
            rule(r"<<").token(T::Punctuation),
            rule(r">>").token(T::Punctuation),
            rule(r"->").token(T::Punctuation),
            rule(r"\|#").token(T::Punctuation),
            rule(r"##").token(T::Punctuation),
            rule(r"\|\*").token(T::Punctuation),
        ]),
        ("unigraph_punctuation", &[
            rule(r"[()\[\]{},.:;|]").token(T::Punctuation),
            rule(r"!").token(T::Punctuation),
            rule(r"\?").token(T::Punctuation),
        ]),
        ("root", &[
            include("whitespace"),
            include("dialecttags"),
            include("pragmas"),
            include("comments"),
            include("identifiers"),
            include("suffixed_number_literals"),
            include("prefixed_number_literals"),
            include("plain_number_literals"),
            include("string_literals"),
            include("digraph_punctuation"),
            include("digraph_operators"),
            include("unigraph_punctuation"),
            include("unigraph_operators"),
        ]),
        ("prefixed_number_literals", &[
            rule(r"0b[01]+(\'[01]+)*").token(T::LiteralNumberBin),
            rule(r"0[ux][0-9A-F]+(\'[0-9A-F]+)*").token(T::LiteralNumberHex),
        ]),
        ("digraph_operators", &[
            rule(r"\*\.").token(T::Operator),
            rule(r"\+>").token(T::Operator),
            rule(r"<>").token(T::Operator),
            rule(r"<=").token(T::Operator),
            rule(r">=").token(T::Operator),
            rule(r"==").token(T::Operator),
            rule(r"::").token(T::Operator),
            rule(r":=").token(T::Operator),
            rule(r"\+\+").token(T::Operator),
            rule(r"--").token(T::Operator),
        ]),
    ],
};
