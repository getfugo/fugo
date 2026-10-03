//! Chroma's `awk.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "awk",
    config: ConfigDef {
        name: "Awk",
        aliases: &["awk", "gawk", "mawk", "nawk"],
        filenames: &["*.awk"],
        mime_types: &["application/x-awk"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^(?=\s|/)").token(T::Text).push(&["slashstartsregex"]),
            include("commentsandwhitespace"),
            rule(r"\+\+|--|\|\||&&|in\b|\$|!?~|\|&|(\*\*|[-<>+*%\^/!=|])=?").token(T::Operator).push(&["slashstartsregex"]),
            rule(r"[{(\[;,]").token(T::Punctuation).push(&["slashstartsregex"]),
            rule(r"[})\].]").token(T::Punctuation),
            rule(r"(break|continue|do|while|exit|for|if|else|return|switch|case|default)\b").token(T::Keyword).push(&["slashstartsregex"]),
            rule(r"function\b").token(T::KeywordDeclaration).push(&["slashstartsregex"]),
            rule(r"(atan2|cos|exp|int|log|rand|sin|sqrt|srand|gensub|gsub|index|length|match|split|patsplit|sprintf|sub|substr|tolower|toupper|close|fflush|getline|next(file)|print|printf|strftime|systime|mktime|delete|system|strtonum|and|compl|lshift|or|rshift|asorti?|isarray|bindtextdomain|dcn?gettext|@(include|load|namespace))\b").token(T::KeywordReserved),
            rule(r"(ARGC|ARGIND|ARGV|BEGIN(FILE)?|BINMODE|CONVFMT|ENVIRON|END(FILE)?|ERRNO|FIELDWIDTHS|FILENAME|FNR|FPAT|FS|IGNORECASE|LINT|NF|NR|OFMT|OFS|ORS|PROCINFO|RLENGTH|RS|RSTART|RT|SUBSEP|TEXTDOMAIN)\b").token(T::NameBuiltin),
            rule(r"[@$a-zA-Z_]\w*").token(T::NameOther),
            rule(r"[0-9][0-9]*\.[0-9]+([eE][0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
        ]),
        ("commentsandwhitespace", &[
            rule(r"\s+").token(T::Text),
            rule(r"#.*$").token(T::CommentSingle),
        ]),
        ("slashstartsregex", &[
            include("commentsandwhitespace"),
            rule(r"/(\\.|[^[/\\\n]|\[(\\.|[^\]\\\n])*])+/\B").token(T::LiteralStringRegex).pop(1),
            rule(r"(?=/)").token(T::Text).push(&["#pop", "badregex"]),
            rule("").pop(1),
        ]),
        ("badregex", &[
            rule(r"\n").token(T::Text).pop(1),
        ]),
    ],
};
