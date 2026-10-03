//! Chroma's `idris.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "idris",
    config: ConfigDef {
        name: "Idris",
        aliases: &["idris", "idr"],
        filenames: &["*.idr"],
        mime_types: &["text/x-idris"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("escape", &[
            rule(r#"[abfnrtv"\'&\\]"#).token(T::LiteralStringEscape).pop(1),
            rule(r"\^[][A-Z@^_]").token(T::LiteralStringEscape).pop(1),
            rule(r"NUL|SOH|[SE]TX|EOT|ENQ|ACK|BEL|BS|HT|LF|VT|FF|CR|S[OI]|DLE|DC[1-4]|NAK|SYN|ETB|CAN|EM|SUB|ESC|[FGRU]S|SP|DEL").token(T::LiteralStringEscape).pop(1),
            rule(r"o[0-7]+").token(T::LiteralStringEscape).pop(1),
            rule(r"x[\da-fA-F]+").token(T::LiteralStringEscape).pop(1),
            rule(r"\d+").token(T::LiteralStringEscape).pop(1),
            rule(r"\s+\\").token(T::LiteralStringEscape).pop(1),
        ]),
        ("root", &[
            rule(r"^(\s*)(%lib|link|flag|include|hide|freeze|access|default|logging|dynamic|name|error_handlers|language)").groups(&[T::Text, T::KeywordReserved]),
            rule(r"(\s*)(--(?![!#$%&*+./<=>?@^|_~:\\]).*?)$").groups(&[T::Text, T::CommentSingle]),
            rule(r"(\s*)(\|{3}.*?)$").groups(&[T::Text, T::CommentSingle]),
            rule(r"(\s*)(\{-)").groups(&[T::Text, T::CommentMultiline]).push(&["comment"]),
            rule(r"^(\s*)([^\s(){}]+)(\s*)(:)(\s*)").groups(&[T::Text, T::NameFunction, T::Text, T::OperatorWord, T::Text]),
            rule(r"\b(case|class|data|default|using|do|else|if|in|infix[lr]?|instance|rewrite|auto|namespace|codata|mutual|private|public|abstract|total|partial|let|proof|of|then|static|where|_|with|pattern|term|syntax|prefix|postulate|parameters|record|dsl|impossible|implicit|tactics|intros|intro|compute|refine|exact|trivial)(?!\')\b").token(T::KeywordReserved),
            rule(r"(import|module)(\s+)").groups(&[T::KeywordReserved, T::Text]).push(&["module"]),
            rule(r"('')?[A-Z][\w\']*").token(T::KeywordType),
            rule(r"[a-z][\w\']*").token(T::Text),
            rule(r"(<-|::|->|=>|=)").token(T::OperatorWord),
            rule(r"([(){}\[\]:!#$%&*+.\\/<=>?@^|~-]+)").token(T::OperatorWord),
            rule(r"\d+[eE][+-]?\d+").token(T::LiteralNumberFloat),
            rule(r"\d+\.\d+([eE][+-]?\d+)?").token(T::LiteralNumberFloat),
            rule(r"0[xX][\da-fA-F]+").token(T::LiteralNumberHex),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r"'").token(T::LiteralStringChar).push(&["character"]),
            rule(r#"""#).token(T::LiteralString).push(&["string"]),
            rule(r"[^\s(){}]+").token(T::Text),
            rule(r"\s+?").token(T::Text),
        ]),
        ("module", &[
            rule(r"\s+").token(T::Text),
            rule(r"([A-Z][\w.]*)(\s+)(\()").groups(&[T::NameNamespace, T::Text, T::Punctuation]).push(&["funclist"]),
            rule(r"[A-Z][\w.]*").token(T::NameNamespace).pop(1),
        ]),
        ("funclist", &[
            rule(r"\s+").token(T::Text),
            rule(r"[A-Z]\w*").token(T::KeywordType),
            rule(r"(_[\w\']+|[a-z][\w\']*)").token(T::NameFunction),
            rule(r"--.*$").token(T::CommentSingle),
            rule(r"\{-").token(T::CommentMultiline).push(&["comment"]),
            rule(r",").token(T::Punctuation),
            rule(r"[:!#$%&*+.\\/<=>?@^|~-]+").token(T::Operator),
            rule(r"\(").token(T::Punctuation).push(&["funclist", "funclist"]),
            rule(r"\)").token(T::Punctuation).pop(2),
        ]),
        ("comment", &[
            rule(r"[^-{}]+").token(T::CommentMultiline),
            rule(r"\{-").token(T::CommentMultiline).push(&[]),
            rule(r"-\}").token(T::CommentMultiline).pop(1),
            rule(r"[-{}]").token(T::CommentMultiline),
        ]),
        ("character", &[
            rule(r"[^\\']").token(T::LiteralStringChar),
            rule(r"\\").token(T::LiteralStringEscape).push(&["escape"]),
            rule(r"'").token(T::LiteralStringChar).pop(1),
        ]),
        ("string", &[
            rule(r#"[^\\"]+"#).token(T::LiteralString),
            rule(r"\\").token(T::LiteralStringEscape).push(&["escape"]),
            rule(r#"""#).token(T::LiteralString).pop(1),
        ]),
    ],
};
