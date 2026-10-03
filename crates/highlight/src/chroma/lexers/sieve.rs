//! Chroma's `sieve.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "sieve",
    config: ConfigDef {
        name: "Sieve",
        aliases: &["sieve"],
        filenames: &["*.siv", "*.sieve"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"[();,{}\[\]]").token(T::Punctuation),
            rule(r"(?i)require").token(T::KeywordNamespace),
            rule(r"(?i)(:)(addresses|all|contains|content|create|copy|comparator|count|days|detail|domain|fcc|flags|from|handle|importance|is|localpart|length|lowerfirst|lower|matches|message|mime|options|over|percent|quotewildcard|raw|regex|specialuse|subject|text|under|upperfirst|upper|value)").groups(&[T::NameTag, T::NameTag]),
            rule(r"(?i)(address|addflag|allof|anyof|body|discard|elsif|else|envelope|ereject|exists|false|fileinto|if|hasflag|header|keep|notify_method_capability|notify|not|redirect|reject|removeflag|setflag|size|spamtest|stop|string|true|vacation|virustest)").token(T::NameBuiltin),
            rule(r"(?i)set").token(T::KeywordDeclaration),
            rule(r"([0-9.]+)([kmgKMG])?").groups(&[T::LiteralNumber, T::LiteralNumber]),
            rule(r"#.*$").token(T::CommentSingle),
            rule(r"/\*.*\*/").token(T::CommentMultiline),
            rule(r#""[^"]*?""#).token(T::LiteralString),
            rule(r"text:").token(T::NameTag).push(&["text"]),
        ]),
        ("text", &[
            rule(r"[^.].*?\n").token(T::LiteralString),
            rule(r"^\.").token(T::Punctuation).pop(1),
        ]),
    ],
};
