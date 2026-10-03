//! Chroma's `reasonml.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "reasonml",
    config: ConfigDef {
        name: "ReasonML",
        aliases: &["reason", "reasonml"],
        filenames: &["*.re", "*.rei"],
        mime_types: &["text/x-reasonml"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("escape-sequence", &[
            rule(r#"\\[\\"\'ntbr]"#).token(T::LiteralStringEscape),
            rule(r"\\[0-9]{3}").token(T::LiteralStringEscape),
            rule(r"\\x[0-9a-fA-F]{2}").token(T::LiteralStringEscape),
        ]),
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"false|true|\(\)|\[\]").token(T::NameBuiltinPseudo),
            rule(r"\b([A-Z][\w\']*)(?=\s*\.)").token(T::NameNamespace).push(&["dotted"]),
            rule(r"\b([A-Z][\w\']*)").token(T::NameClass),
            rule(r"//.*?\n").token(T::CommentSingle),
            rule(r"\/\*(?![\/])").token(T::CommentMultiline).push(&["comment"]),
            rule(r"\b(as|assert|begin|class|constraint|do|done|downto|else|end|exception|external|false|for|fun|esfun|function|functor|if|in|include|inherit|initializer|lazy|let|switch|module|pub|mutable|new|nonrec|object|of|open|pri|rec|sig|struct|then|to|true|try|type|val|virtual|when|while|with)\b").token(T::Keyword),
            rule(r"(~|\}|\|]|\||\|\||\{<|\{|`|_|]|\[\||\[>|\[<|\[|\?\?|\?|>\}|>]|>|=|<-|<|;;|;|:>|:=|::|:|\.\.\.|\.\.|\.|=>|-\.|-|,|\+|\*|\)|\(|&&|&|#|!=)").token(T::OperatorWord),
            rule(r"([=<>@^|&+\*/$%-]|[!?~])?[!$%&*+\./:<=>?@^|~-]").token(T::Operator),
            rule(r"\b(and|asr|land|lor|lsl|lsr|lxor|mod|or)\b").token(T::OperatorWord),
            rule(r"\b(unit|int|float|bool|string|char|list|array)\b").token(T::KeywordType),
            rule(r"[^\W\d][\w']*").token(T::Name),
            rule(r"-?\d[\d_]*(.[\d_]*)?([eE][+\-]?\d[\d_]*)").token(T::LiteralNumberFloat),
            rule(r"0[xX][\da-fA-F][\da-fA-F_]*").token(T::LiteralNumberHex),
            rule(r"0[oO][0-7][0-7_]*").token(T::LiteralNumberOct),
            rule(r"0[bB][01][01_]*").token(T::LiteralNumberBin),
            rule(r"\d[\d_]*").token(T::LiteralNumberInteger),
            rule(r#"'(?:(\\[\\\"'ntbr ])|(\\[0-9]{3})|(\\x[0-9a-fA-F]{2}))'"#).token(T::LiteralStringChar),
            rule(r"'.'").token(T::LiteralStringChar),
            rule(r"'").token(T::Keyword),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"[~?][a-z][\w\']*:").token(T::NameVariable),
        ]),
        ("comment", &[
            rule(r"[^\/*]+").token(T::CommentMultiline),
            rule(r"\/\*").token(T::CommentMultiline).push(&[]),
            rule(r"\*\/").token(T::CommentMultiline).pop(1),
            rule(r"[\*]").token(T::CommentMultiline),
        ]),
        ("string", &[
            rule(r#"[^\\"]+"#).token(T::LiteralStringDouble),
            include("escape-sequence"),
            rule(r"\\\n").token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
        ("dotted", &[
            rule(r"\s+").token(T::Text),
            rule(r"\.").token(T::Punctuation),
            rule(r"[A-Z][\w\']*(?=\s*\.)").token(T::NameNamespace),
            rule(r"[A-Z][\w\']*").token(T::NameClass).pop(1),
            rule(r"[a-z_][\w\']*").token(T::Name).pop(1),
            rule("").pop(1),
        ]),
    ],
};
