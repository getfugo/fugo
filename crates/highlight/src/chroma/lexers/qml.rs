//! Chroma's `qml.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "qml",
    config: ConfigDef {
        name: "QML",
        aliases: &["qml", "qbs"],
        filenames: &["*.qml", "*.qbs"],
        mime_types: &["application/x-qml", "application/x-qt.qbs+qml"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^(?=\s|/|<!--)").token(T::Text).push(&["slashstartsregex"]),
            include("commentsandwhitespace"),
            rule(r"\+\+|--|~|&&|\?|:|\|\||\\(?=\n)|(<<|>>>?|==?|!=?|[-<>+*%&|^/])=?").token(T::Operator).push(&["slashstartsregex"]),
            rule(r"[{(\[;,]").token(T::Punctuation).push(&["slashstartsregex"]),
            rule(r"[})\].]").token(T::Punctuation),
            rule(r"\bid\s*:\s*[A-Za-z][\w.]*").token(T::KeywordDeclaration).push(&["slashstartsregex"]),
            rule(r"\b[A-Za-z][\w.]*\s*:").token(T::Keyword).push(&["slashstartsregex"]),
            rule(r"(for|in|while|do|break|return|continue|switch|case|default|if|else|throw|try|catch|finally|new|delete|typeof|instanceof|void|this)\b").token(T::Keyword).push(&["slashstartsregex"]),
            rule(r"(var|let|with|function)\b").token(T::KeywordDeclaration).push(&["slashstartsregex"]),
            rule(r"(abstract|boolean|byte|char|class|const|debugger|double|enum|export|extends|final|float|goto|implements|import|int|interface|long|native|package|private|protected|public|short|static|super|synchronized|throws|transient|volatile)\b").token(T::KeywordReserved),
            rule(r"(true|false|null|NaN|Infinity|undefined)\b").token(T::KeywordConstant),
            rule(r"(Array|Boolean|Date|Error|Function|Math|netscape|Number|Object|Packages|RegExp|String|sun|decodeURI|decodeURIComponent|encodeURI|encodeURIComponent|Error|eval|isFinite|isNaN|parseFloat|parseInt|document|this|window)\b").token(T::NameBuiltin),
            rule(r"[$a-zA-Z_]\w*").token(T::NameOther),
            rule(r"[0-9][0-9]*\.[0-9]+([eE][0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
        ]),
        ("commentsandwhitespace", &[
            rule(r"\s+").token(T::Text),
            rule(r"<!--").token(T::Comment),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
        ]),
        ("slashstartsregex", &[
            include("commentsandwhitespace"),
            rule(r"/(\\.|[^[/\\\n]|\[(\\.|[^\]\\\n])*])+/([gim]+\b|\B)").token(T::LiteralStringRegex).pop(1),
            rule(r"(?=/)").token(T::Text).push(&["#pop", "badregex"]),
            rule("").pop(1),
        ]),
        ("badregex", &[
            rule(r"\n").token(T::Text).pop(1),
        ]),
    ],
};
