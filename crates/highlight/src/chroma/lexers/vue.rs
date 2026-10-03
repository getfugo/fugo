//! Chroma's `vue.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "vue",
    config: ConfigDef {
        name: "vue",
        aliases: &["vue", "vuejs"],
        filenames: &["*.vue"],
        mime_types: &["text/x-vue", "application/x-vue"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("interp-inside", &[
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
            include("root"),
        ]),
        ("attr", &[
            rule(r"{").token(T::Punctuation).push(&["expression"]),
            rule(r#"".*?""#).token(T::LiteralString).pop(1),
            rule(r"'.*?'").token(T::LiteralString).pop(1),
            rule("").pop(1),
        ]),
        ("interp", &[
            rule(r"`").token(T::LiteralStringBacktick).pop(1),
            rule(r"\\\\").token(T::LiteralStringBacktick),
            rule(r"\\`").token(T::LiteralStringBacktick),
            rule(r"\$\{").token(T::LiteralStringInterpol).push(&["interp-inside"]),
            rule(r"\$").token(T::LiteralStringBacktick),
            rule(r"[^`\\$]+").token(T::LiteralStringBacktick),
        ]),
        ("tag", &[
            rule(r"\s+").token(T::Text),
            rule(r"(-)([\w]+)").token(T::NameTag),
            rule(r#"(@[\w.]+)(=".*?")?(\/?>|\s)"#).groups(&[T::NameTag, T::LiteralString, T::Punctuation]),
            rule(r#"(:[\S]+)(=)("[\S]+")"#).groups(&[T::NameTag, T::Operator, T::LiteralString]),
            rule(r"(:)").token(T::Operator),
            rule(r"(v-b-[\S]+)").token(T::NameTag),
            rule(r#"(v-[\w-]+)(=)("[\S ]+")(\/?>|\s)"#).groups(&[T::NameTag, T::Operator, T::LiteralString, T::Punctuation]),
            rule(r"(v-[\w-]+)(\/?>|\s)").groups(&[T::NameTag, T::Punctuation]),
            rule(r#"(v-[\w-]+)(=".+")(\/?>|\s)"#).groups(&[T::NameTag, T::LiteralString, T::Punctuation]),
            rule(r#"(v-[\w-]+)(=".+)([:][\w]+)(="[\w]+")(\/?>|\s)"#).groups(&[T::NameTag, T::LiteralString, T::NameTag, T::LiteralString, T::Punctuation]),
            rule(r"(<)([\w]+)").groups(&[T::Punctuation, T::NameTag]),
            rule(r"(<)(/)([\w]+)(>)").groups(&[T::Punctuation, T::Punctuation, T::NameTag, T::Punctuation]),
            rule(r"([\w]+\s*)(=)(\s*)").groups(&[T::NameAttribute, T::Operator, T::Text]).push(&["attr"]),
            rule(r"[{}]+").token(T::Punctuation),
            rule(r"[\w\.]+").token(T::NameAttribute),
            rule(r"(/?)(\s*)(>)").groups(&[T::Punctuation, T::Text, T::Punctuation]).pop(1),
        ]),
        ("slashstartsregex", &[
            include("commentsandwhitespace"),
            rule(r"/(\\.|[^[/\\\n]|\[(\\.|[^\]\\\n])*])+/([gimuy]+\b|\B)").token(T::LiteralStringRegex).pop(1),
            rule(r"(?=/)").token(T::Text).push(&["#pop", "badregex"]),
            rule("").pop(1),
        ]),
        ("root", &[
            include("vue"),
            rule(r"\A#! ?/.*?\n").token(T::CommentHashbang),
            rule(r"^(?=\s|/|<!--)").token(T::Text).push(&["slashstartsregex"]),
            include("commentsandwhitespace"),
            rule(r"(\.\d+|[0-9]+\.[0-9]*)([eE][-+]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"0[bB][01]+").token(T::LiteralNumberBin),
            rule(r"0[oO][0-7]+").token(T::LiteralNumberOct),
            rule(r"0[xX][0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r"\.\.\.|=>").token(T::Punctuation),
            rule(r"\+\+|--|~|&&|\?|:|\|\||\\(?=\n)|(<<|>>>?|==?|!=?|[-<>+*%&|^/])=?").token(T::Operator).push(&["slashstartsregex"]),
            rule(r"[{(\[;,]").token(T::Punctuation).push(&["slashstartsregex"]),
            rule(r"[})\].]").token(T::Punctuation),
            rule(r"(for|in|while|do|break|return|continue|switch|case|default|if|else|throw|try|catch|finally|new|delete|typeof|instanceof|void|yield|this|of)\b").token(T::Keyword).push(&["slashstartsregex"]),
            rule(r"(var|let|with|function)\b").token(T::KeywordDeclaration).push(&["slashstartsregex"]),
            rule(r"(abstract|boolean|byte|char|class|const|debugger|double|enum|export|extends|final|float|goto|implements|import|int|interface|long|native|package|private|protected|public|short|static|super|synchronized|throws|transient|volatile)\b").token(T::KeywordReserved),
            rule(r"(true|false|null|NaN|Infinity|undefined)\b").token(T::KeywordConstant),
            rule(r"(Array|Boolean|Date|Error|Function|Math|netscape|Number|Object|Packages|RegExp|String|Promise|Proxy|sun|decodeURI|decodeURIComponent|encodeURI|encodeURIComponent|Error|eval|isFinite|isNaN|isSafeInteger|parseFloat|parseInt|document|this|window)\b").token(T::NameBuiltin),
            rule(r"(?:[$_\p{L}\p{N}]|\\u[a-fA-F0-9]{4})(?:(?:[$\p{L}\p{N}]|\\u[a-fA-F0-9]{4}))*").token(T::NameOther),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"`").token(T::LiteralStringBacktick).push(&["interp"]),
        ]),
        ("badregex", &[
            rule(r"\n").token(T::Text).pop(1),
        ]),
        ("vue", &[
            rule(r"(<)([\w-]+)").groups(&[T::Punctuation, T::NameTag]).push(&["tag"]),
            rule(r"(<)(/)([\w-]+)(>)").groups(&[T::Punctuation, T::Punctuation, T::NameTag, T::Punctuation]),
        ]),
        ("expression", &[
            rule(r"{").token(T::Punctuation).push(&[]),
            rule(r"}").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("commentsandwhitespace", &[
            rule(r"\s+").token(T::Text),
            rule(r"<!--").token(T::Comment),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
        ]),
    ],
};
