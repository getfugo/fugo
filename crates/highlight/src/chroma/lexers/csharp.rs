//! Chroma's `c#.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "c#",
    config: ConfigDef {
        name: "C#",
        aliases: &["csharp", "c#"],
        filenames: &["*.cs"],
        mime_types: &["text/x-csharp"],
        dot_all: true,
        ensure_nl: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^\s*\[.*?\]").token(T::NameAttribute),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\\\n").token(T::Text),
            rule(r"///[^\n\r]*").token(T::CommentSpecial),
            rule(r"//[^\n\r]*").token(T::CommentSingle),
            rule(r"/[*].*?[*]/").token(T::CommentMultiline),
            rule(r"\n").token(T::Text),
            rule(r"[~!%^&*()+=|\[\]:;,.<>/?-]").token(T::Punctuation),
            rule(r"[{}]").token(T::Punctuation),
            rule(r#"@"(""|[^"])*""#).token(T::LiteralString),
            rule(r#"\$@?"(""|[^"])*""#).token(T::LiteralString),
            rule(r#""(\\\\|\\"|[^"\n])*["\n]"#).token(T::LiteralString),
            rule(r"'\\.'|'[^\\]'").token(T::LiteralStringChar),
            rule(r"0[xX][0-9a-fA-F]+[Ll]?|\d[_\d]*(\.\d*)?([eE][+-]?\d+)?[flFLdD]?").token(T::LiteralNumber),
            rule(r"#[ \t]*(if|endif|else|elif|define|undef|line|error|warning|region|endregion|pragma|nullable)\b").token(T::CommentPreproc),
            rule(r"\b(extern)(\s+)(alias)\b").groups(&[T::Keyword, T::Text, T::Keyword]),
            rule(r"(as|await|base|break|by|case|catch|checked|continue|default|delegate|do|else|event|finally|fixed|for|foreach|goto|if|in|init|is|let|lock|new|on|out|params|readonly|ref|return|sizeof|stackalloc|switch|this|throw|try|typeof|unchecked|virtual|void|while|get|set|new|yield|add|remove|value|alias|ascending|descending|from|group|into|orderby|select|thenby|where|join|equals)\b").token(T::Keyword),
            rule(r"(global)(::)").groups(&[T::Keyword, T::Punctuation]),
            rule(r"(abstract|async|const|enum|explicit|extern|implicit|internal|operator|override|partial|private|protected|public|static|sealed|unsafe|volatile)\b").token(T::KeywordDeclaration),
            rule(r"(bool|byte|char|decimal|double|dynamic|float|int|long|object|sbyte|short|string|uint|ulong|ushort|var)\b\??").token(T::KeywordType),
            rule(r"(true|false|null)\b").token(T::KeywordConstant),
            rule(r"(class|struct|record|interface)(\s+)").groups(&[T::Keyword, T::Text]).push(&["class"]),
            rule(r"(namespace|using)(\s+)").groups(&[T::Keyword, T::Text]).push(&["namespace"]),
            rule(r"@?[_a-zA-Z]\w*").token(T::Name),
        ]),
        ("class", &[
            rule(r"@?[_a-zA-Z]\w*").token(T::NameClass).pop(1),
            rule("").pop(1),
        ]),
        ("namespace", &[
            rule(r"(?=\()").token(T::Text).pop(1),
            rule(r"(@?[_a-zA-Z]\w*|\.)+").token(T::NameNamespace).pop(1),
        ]),
    ],
};
