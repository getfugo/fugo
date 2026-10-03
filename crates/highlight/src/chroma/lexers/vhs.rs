//! Chroma's `vhs.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "vhs",
    config: ConfigDef {
        name: "VHS",
        aliases: &["vhs", "tape", "cassette"],
        filenames: &["*.tape"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(Output)(\s+)(.*)(\s+)").groups(&[T::Keyword, T::TextWhitespace, T::LiteralString, T::TextWhitespace]),
            rule(r"\b(Set|Type|Left|Right|Up|Down|Backspace|Enter|Tab|Space|Ctrl|Sleep|Hide|Show|Escape)\b").token(T::Keyword),
            rule(r"\b(FontFamily|FontSize|Framerate|Height|Width|Theme|Padding|TypingSpeed|PlaybackSpeed|LineHeight|Framerate|LetterSpacing)\b").token(T::NameBuiltin),
            rule(r"#.*(\S|$)").token(T::Comment),
            rule(r#"(?s)".*""#).token(T::LiteralStringDouble),
            rule(r"(?s)'.*'").token(T::LiteralStringSingle),
            rule(r"(@|\+)").token(T::Punctuation),
            rule(r"\d+").token(T::LiteralNumber),
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"(ms|s)").token(T::Text),
        ]),
    ],
};
