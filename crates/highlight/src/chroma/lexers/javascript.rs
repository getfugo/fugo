//! Chroma's `javascript.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "javascript",
    config: ConfigDef {
        name: "JavaScript",
        aliases: &["js", "javascript"],
        filenames: &["*.js", "*.jsm", "*.mjs", "*.cjs"],
        mime_types: &[
            "application/javascript",
            "application/x-javascript",
            "text/x-javascript",
            "text/javascript",
        ],
        dot_all: true,
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("interp", &[
            rule(r"`").token(T::LiteralStringBacktick).pop(1),
            rule(r"\\\\").token(T::LiteralStringBacktick),
            rule(r"\\`").token(T::LiteralStringBacktick),
            rule(r"\\[^`\\]").token(T::LiteralStringBacktick),
            rule(r"\$\{").token(T::LiteralStringInterpol).push(&["interp-inside"]),
            rule(r"\$").token(T::LiteralStringBacktick),
            rule(r"[^`\\$]+").token(T::LiteralStringBacktick),
        ]),
        ("interp-inside", &[
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
            include("root"),
        ]),
        ("commentsandwhitespace", &[
            rule(r"\s+").token(T::Text),
            rule(r"<!--").token(T::Comment),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
        ]),
        ("slashstartsregex", &[
            include("commentsandwhitespace"),
            rule(r"/(\\.|[^[/\\\n]|\[(\\.|[^\]\\\n])*])+/([gimuy]+\b|\B)").token(T::LiteralStringRegex).pop(1),
            rule(r"(?=/)").token(T::Text).push(&["#pop", "badregex"]),
            rule("").pop(1),
        ]),
        ("badregex", &[
            rule(r"\n").token(T::Text).pop(1),
        ]),
        ("root", &[
            rule(r"\A#! ?/.*?\n").token(T::CommentHashbang),
            rule(r"^(?=\s|/|<!--)").token(T::Text).push(&["slashstartsregex"]),
            include("commentsandwhitespace"),
            rule(r"\d+(\.\d*|[eE][+\-]?\d+)").token(T::LiteralNumberFloat),
            rule(r"0[bB][01]+").token(T::LiteralNumberBin),
            rule(r"0[oO][0-7]+").token(T::LiteralNumberOct),
            rule(r"0[xX][0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"[0-9][0-9_]*").token(T::LiteralNumberInteger),
            rule(r"\.\.\.|=>").token(T::Punctuation),
            rule(r"\+\+|--|~|&&|\?|:|\|\||\\(?=\n)|(<<|>>>?|==?|!=?|[-<>+*%&|^/])=?").token(T::Operator).push(&["slashstartsregex"]),
            rule(r"[{(\[;,]").token(T::Punctuation).push(&["slashstartsregex"]),
            rule(r"[})\].]").token(T::Punctuation),
            rule(r"(for|in|while|do|break|return|continue|switch|case|default|if|else|throw|try|catch|finally|new|delete|typeof|instanceof|void|yield|this|of)\b").token(T::Keyword).push(&["slashstartsregex"]),
            rule(r"(var|let|with|function)\b").token(T::KeywordDeclaration).push(&["slashstartsregex"]),
            rule(r"(abstract|async|await|boolean|byte|char|class|const|debugger|double|enum|export|extends|final|float|goto|implements|import|int|interface|long|native|package|private|protected|public|short|static|super|synchronized|throws|transient|volatile)\b").token(T::KeywordReserved),
            rule(r"(true|false|null|NaN|Infinity|undefined)\b").token(T::KeywordConstant),
            rule(r"(Array|Boolean|Date|Error|Function|Math|netscape|Number|Object|Packages|RegExp|String|Promise|Proxy|sun|decodeURI|decodeURIComponent|encodeURI|encodeURIComponent|Error|eval|isFinite|isNaN|isSafeInteger|parseFloat|parseInt|document|this|window)\b").token(T::NameBuiltin),
            rule(r"(?:[$_\p{L}\p{N}]|\\u[a-fA-F0-9]{4})(?:(?:[$\p{L}\p{N}]|\\u[a-fA-F0-9]{4}))*").token(T::NameOther),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"`").token(T::LiteralStringBacktick).push(&["interp"]),
        ]),
    ],
};
