//! Chroma's `actionscript_3.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "actionscript_3",
    config: ConfigDef {
        name: "ActionScript 3",
        aliases: &["as3", "actionscript3"],
        filenames: &["*.as"],
        mime_types: &["application/x-actionscript3", "text/x-actionscript3", "text/actionscript3"],
        dot_all: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("funcparams", &[
            rule(r"\s+").token(T::Text),
            rule(r"(\s*)(\.\.\.)?([$a-zA-Z_]\w*)(\s*)(:)(\s*)([$a-zA-Z_]\w*(?:\.<\w+>)?|\*)(\s*)").groups(&[T::Text, T::Punctuation, T::Name, T::Text, T::Operator, T::Text, T::KeywordType, T::Text]).push(&["defval"]),
            rule(r"\)").token(T::Operator).push(&["type"]),
        ]),
        ("type", &[
            rule(r"(\s*)(:)(\s*)([$a-zA-Z_]\w*(?:\.<\w+>)?|\*)").groups(&[T::Text, T::Operator, T::Text, T::KeywordType]).pop(2),
            rule(r"\s+").token(T::Text).pop(2),
            rule("").pop(2),
        ]),
        ("defval", &[
            rule(r"(=)(\s*)([^(),]+)(\s*)(,?)").bygroups(&[E::Token(T::Operator), E::Token(T::Text), E::UsingSelf("root"), E::Token(T::Text), E::Token(T::Operator)]).pop(1),
            rule(r",").token(T::Operator).pop(1),
            rule("").pop(1),
        ]),
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"(function\s+)([$a-zA-Z_]\w*)(\s*)(\()").groups(&[T::KeywordDeclaration, T::NameFunction, T::Text, T::Operator]).push(&["funcparams"]),
            rule(r"(var|const)(\s+)([$a-zA-Z_]\w*)(\s*)(:)(\s*)([$a-zA-Z_]\w*(?:\.<\w+>)?)").groups(&[T::KeywordDeclaration, T::Text, T::Name, T::Text, T::Punctuation, T::Text, T::KeywordType]),
            rule(r"(import|package)(\s+)((?:[$a-zA-Z_]\w*|\.)+)(\s*)").groups(&[T::Keyword, T::Text, T::NameNamespace, T::Text]),
            rule(r"(new)(\s+)([$a-zA-Z_]\w*(?:\.<\w+>)?)(\s*)(\()").groups(&[T::Keyword, T::Text, T::KeywordType, T::Text, T::Operator]),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"/\*.*?\*/").token(T::CommentMultiline),
            rule(r"/(\\\\|\\/|[^\n])*/[gisx]*").token(T::LiteralStringRegex),
            rule(r"(\.)([$a-zA-Z_]\w*)").groups(&[T::Operator, T::NameAttribute]),
            rule(r"(case|default|for|each|in|while|do|break|return|continue|if|else|throw|try|catch|with|new|typeof|arguments|instanceof|this|switch|import|include|as|is)\b").token(T::Keyword),
            rule(r"(class|public|final|internal|native|override|private|protected|static|import|extends|implements|interface|intrinsic|return|super|dynamic|function|const|get|namespace|package|set)\b").token(T::KeywordDeclaration),
            rule(r"(true|false|null|NaN|Infinity|-Infinity|undefined|void)\b").token(T::KeywordConstant),
            rule(r"(decodeURI|decodeURIComponent|encodeURI|escape|eval|isFinite|isNaN|isXMLName|clearInterval|fscommand|getTimer|getURL|getVersion|isFinite|parseFloat|parseInt|setInterval|trace|updateAfterEvent|unescape)\b").token(T::NameFunction),
            rule(r"[$a-zA-Z_]\w*").token(T::Name),
            rule(r"[0-9][0-9]*\.[0-9]+([eE][0-9]+)?[fd]?").token(T::LiteralNumberFloat),
            rule(r"0x[0-9a-f]+").token(T::LiteralNumberHex),
            rule(r"[0-9]+").token(T::LiteralNumberInteger),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"'(\\\\|\\'|[^'])*'").token(T::LiteralStringSingle),
            rule(r"[~^*!%&<>|+=:;,/?\\{}\[\]().-]+").token(T::Operator),
        ]),
    ],
};
