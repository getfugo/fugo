//! Chroma's `typescript.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "typescript",
    config: ConfigDef {
        name: "TypeScript",
        aliases: &["ts", "tsx", "typescript"],
        filenames: &["*.ts", "*.tsx", "*.mts", "*.cts"],
        mime_types: &["text/x-typescript"],
        dot_all: true,
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("expression", &[
            rule(r"{").token(T::Punctuation).push(&[]),
            rule(r"}").token(T::Punctuation).pop(1),
            include("root"),
        ]),
        ("jsx", &[
            rule(r"(<)(/?)(>)").groups(&[T::Punctuation, T::Punctuation, T::Punctuation]),
            rule(r"(<)([\w\.]+)").groups(&[T::Punctuation, T::NameTag]).push(&["tag"]),
            rule(r"(<)(/)([\w\.]*)(>)").groups(&[T::Punctuation, T::Punctuation, T::NameTag, T::Punctuation]),
        ]),
        ("tag", &[
            include("jsx"),
            rule(r",").token(T::Punctuation),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"`").token(T::LiteralStringBacktick).push(&["interp"]),
            include("commentsandwhitespace"),
            rule(r"\s+").token(T::Text),
            rule(r"([\w-]+\s*)(=)(\s*)").groups(&[T::NameAttribute, T::Operator, T::Text]).push(&["attr"]),
            rule(r"[{}]+").token(T::Punctuation),
            rule(r"[\w\.]+").token(T::NameAttribute),
            rule(r"(/?)(\s*)(>)").groups(&[T::Punctuation, T::Text, T::Punctuation]).pop(1),
        ]),
        ("comment", &[
            rule(r"[^-]+").token(T::Comment),
            rule(r"-->").token(T::Comment).pop(1),
            rule(r"-").token(T::Comment),
        ]),
        ("commentsandwhitespace", &[
            rule(r"\s+").token(T::Text),
            rule(r"<!--").token(T::Comment).push(&["comment"]),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
        ]),
        ("badregex", &[
            rule(r"\n").token(T::Text).pop(1),
        ]),
        ("interp", &[
            rule(r"`").token(T::LiteralStringBacktick).pop(1),
            rule(r"\\\\").token(T::LiteralStringBacktick),
            rule(r"\\`").token(T::LiteralStringBacktick),
            rule(r"\$\{").token(T::LiteralStringInterpol).push(&["interp-inside"]),
            rule(r"\$").token(T::LiteralStringBacktick),
            rule(r"[^`\\$]+").token(T::LiteralStringBacktick),
        ]),
        ("attr", &[
            rule(r"{").token(T::Punctuation).push(&["expression"]),
            rule(r#"".*?""#).token(T::LiteralString).pop(1),
            rule(r"'.*?'").token(T::LiteralString).pop(1),
            rule("").pop(1),
        ]),
        ("interp-inside", &[
            rule(r"\}").token(T::LiteralStringInterpol).pop(1),
            include("root"),
        ]),
        ("slashstartsregex", &[
            include("commentsandwhitespace"),
            rule(r"/(\\.|[^[/\\\n]|\[(\\.|[^\]\\\n])*])+/([gim]+\b|\B)").token(T::LiteralStringRegex).pop(1),
            rule(r"(?=/)").token(T::Text).push(&["badregex"]),
            rule("").pop(1),
        ]),
        ("root", &[
            include("jsx"),
            rule(r"^(?=\s|/|<!--)").token(T::Text).push(&["slashstartsregex"]),
            include("commentsandwhitespace"),
            rule(r"\+\+|--|~|&&|\?|:|\|\||\\(?=\n)|(<<|>>>?|==?|!=?|[-<>+*%&|^/])=?").token(T::Operator).push(&["slashstartsregex"]),
            rule(r"[{(\[;,]").token(T::Punctuation).push(&["slashstartsregex"]),
            rule(r"[})\].]").token(T::Punctuation),
            rule(r"(for|in|of|while|do|break|return|yield|continue|switch|case|default|if|else|throw|try|catch|finally|new|delete|typeof|instanceof|keyof|asserts|is|infer|await|void|this)\b").token(T::Keyword).push(&["slashstartsregex"]),
            rule(r"(var|let|with|function)\b").token(T::KeywordDeclaration).push(&["slashstartsregex"]),
            rule(r"(abstract|async|boolean|class|const|debugger|enum|export|extends|from|get|global|goto|implements|import|interface|namespace|package|private|protected|public|readonly|require|set|static|super|type)\b").token(T::KeywordReserved),
            rule(r"(true|false|null|NaN|Infinity|undefined)\b").token(T::KeywordConstant),
            rule(r"(Array|Boolean|Date|Error|Function|Math|Number|Object|Packages|RegExp|String|decodeURI|decodeURIComponent|encodeURI|encodeURIComponent|eval|isFinite|isNaN|parseFloat|parseInt|document|this|window)\b").token(T::NameBuiltin),
            rule(r#"\b(module)(\s+)("[\w\./@]+")(\s+)"#).groups(&[T::KeywordReserved, T::Text, T::NameOther, T::Text]).push(&["slashstartsregex"]),
            rule(r"\b(string|bool|number|any|never|object|symbol|unique|unknown|bigint)\b").token(T::KeywordType),
            rule(r"\b(constructor|declare|interface|as)\b").token(T::KeywordReserved),
            rule(r"(super)(\s*)(\([\w,?.$\s]+\s*\))").groups(&[T::KeywordReserved, T::TextWhitespace, T::Text]).push(&["slashstartsregex"]),
            rule(r"([a-zA-Z_?.$][\w?.$]*)\(\) \{").token(T::NameOther).push(&["slashstartsregex"]),
            rule(r"([\w?.$][\w?.$]*)(\s*:\s*)([\w?.$][\w?.$]*)").groups(&[T::NameOther, T::Text, T::KeywordType]),
            rule(r"[$a-zA-Z_]\w*").token(T::NameOther),
            rule(r"[0-9][0-9]*\.[0-9]+([eE][0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-fA-F]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"`").token(T::LiteralStringBacktick).push(&["interp"]),
            rule(r"@\w+").token(T::KeywordDeclaration),
        ]),
    ],
};
