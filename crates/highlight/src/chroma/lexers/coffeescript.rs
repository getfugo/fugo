//! Chroma's `coffeescript.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "coffeescript",
    config: ConfigDef {
        name: "CoffeeScript",
        aliases: &["coffee-script", "coffeescript", "coffee"],
        filenames: &["*.coffee"],
        mime_types: &["text/coffeescript"],
        dot_all: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("commentsandwhitespace", &[
            rule(r"\s+").token(T::Text),
            rule(r"###[^#].*?###").token(T::CommentMultiline),
            rule(r"#(?!##[^#]).*?\n").token(T::CommentSingle),
        ]),
        ("multilineregex", &[
            rule(r"[^/#]+").token(T::LiteralStringRegex),
            rule(r"///([gim]+\b|\B)").token(T::LiteralStringRegex).pop(1),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpoling_string"]),
            rule(r"[/#]").token(T::LiteralStringRegex),
        ]),
        ("slashstartsregex", &[
            include("commentsandwhitespace"),
            rule(r"///").token(T::LiteralStringRegex).push(&["#pop", "multilineregex"]),
            rule(r"/(?! )(\\.|[^[/\\\n]|\[(\\.|[^\]\\\n])*])+/([gim]+\b|\B)").token(T::LiteralStringRegex).pop(1),
            rule(r"/").token(T::Operator),
            rule("").pop(1),
        ]),
        ("tsqs", &[
            rule(r"'''").token(T::LiteralString).pop(1),
            rule(r#"#|\\.|\'|""#).token(T::LiteralString),
            include("strings"),
        ]),
        ("dqs", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r"\\.|\'").token(T::LiteralString),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpoling_string"]),
            rule(r"#").token(T::LiteralString),
            include("strings"),
        ]),
        ("sqs", &[
            rule(r"'").token(T::LiteralString).pop(1),
            rule(r#"#|\\.|""#).token(T::LiteralString),
            include("strings"),
        ]),
        ("tdqs", &[
            rule(r#"""""#).token(T::LiteralString).pop(1),
            rule(r#"\\.|\'|""#).token(T::LiteralString),
            rule(r"#\{").token(T::LiteralStringInterpol).push(&["interpoling_string"]),
            rule(r"#").token(T::LiteralString),
            include("strings"),
        ]),
        ("root", &[
            include("commentsandwhitespace"),
            rule(r"^(?=\s|/)").token(T::Text).push(&["slashstartsregex"]),
            rule(r"\+\+|~|&&|\band\b|\bor\b|\bis\b|\bisnt\b|\bnot\b|\?|:|\|\||\\(?=\n)|(<<|>>>?|==?(?!>)|!=?|=(?!>)|-(?!>)|[<>+*`%&\|\^/])=?").token(T::Operator).push(&["slashstartsregex"]),
            rule(r"(?:\([^()]*\))?\s*[=-]>").token(T::NameFunction).push(&["slashstartsregex"]),
            rule(r"[{(\[;,]").token(T::Punctuation).push(&["slashstartsregex"]),
            rule(r"[})\].]").token(T::Punctuation),
            rule(r"(?<![.$])(for|own|in|of|while|until|loop|break|return|continue|switch|when|then|if|unless|else|throw|try|catch|finally|new|delete|typeof|instanceof|super|extends|this|class|by)\b").token(T::Keyword).push(&["slashstartsregex"]),
            rule(r"(?<![.$])(true|false|yes|no|on|off|null|NaN|Infinity|undefined)\b").token(T::KeywordConstant),
            rule(r"(Array|Boolean|Date|Error|Function|Math|netscape|Number|Object|Packages|RegExp|String|sun|decodeURI|decodeURIComponent|encodeURI|encodeURIComponent|eval|isFinite|isNaN|parseFloat|parseInt|document|window)\b").token(T::NameBuiltin),
            rule(r"[$a-zA-Z_][\w.:$]*\s*[:=]\s").token(T::NameVariable).push(&["slashstartsregex"]),
            rule(r"@[$a-zA-Z_][\w.:$]*\s*[:=]\s").token(T::NameVariableInstance).push(&["slashstartsregex"]),
            rule(r"@").token(T::NameOther).push(&["slashstartsregex"]),
            rule(r"@?[$a-zA-Z_][\w$]*").token(T::NameOther),
            rule(r"[0-9][0-9]*\.[0-9]+([eE][0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r#"""""#).token(T::LiteralString).push(&["tdqs"]),
            rule(r"'''").token(T::LiteralString).push(&["tsqs"]),
            rule(r#"""#).token(T::LiteralString).push(&["dqs"]),
            rule(r"'").token(T::LiteralString).push(&["sqs"]),
        ]),
        ("interpoling_string", &[
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
            include("root"),
        ]),
        ("strings", &[
            rule(r#"[^#\\\'"]+"#).token(T::LiteralString),
        ]),
    ],
};
