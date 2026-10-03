//! Chroma's `cmake.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "cmake",
    config: ConfigDef {
        name: "CMake",
        aliases: &["cmake"],
        filenames: &["*.cmake", "CMakeLists.txt"],
        mime_types: &["text/x-cmake"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\b(\w+)([ \t]*)(\()").groups(&[T::NameBuiltin, T::Text, T::Punctuation]).push(&["args"]),
            include("keywords"),
            include("ws"),
        ]),
        ("args", &[
            rule(r"\(").token(T::Punctuation).push(&[]),
            rule(r"\)").token(T::Punctuation).pop(1),
            rule(r"(\$\{)(.+?)(\})").groups(&[T::Operator, T::NameVariable, T::Operator]),
            rule(r"(\$ENV\{)(.+?)(\})").groups(&[T::Operator, T::NameVariable, T::Operator]),
            rule(r"(\$<)(.+?)(>)").groups(&[T::Operator, T::NameVariable, T::Operator]),
            rule(r#"(?s)".*?""#).token(T::LiteralStringDouble),
            rule(r"\\\S+").token(T::LiteralString),
            rule(r##"[^)$"# \t\n]+"##).token(T::LiteralString),
            rule(r"\n").token(T::Text),
            include("keywords"),
            include("ws"),
        ]),
        ("string", &[]),
        ("keywords", &[
            rule(r"\b(WIN32|UNIX|APPLE|CYGWIN|BORLAND|MINGW|MSVC|MSVC_IDE|MSVC60|MSVC70|MSVC71|MSVC80|MSVC90)\b").token(T::Keyword),
        ]),
        ("ws", &[
            rule(r"[ \t]+").token(T::Text),
            rule(r"#.*\n").token(T::Comment),
        ]),
    ],
};
