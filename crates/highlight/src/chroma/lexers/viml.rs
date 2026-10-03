//! Chroma's `viml.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "viml",
    config: ConfigDef {
        name: "VimL",
        aliases: &["vim"],
        filenames: &[
            "*.vim",
            ".vimrc",
            ".exrc",
            ".gvimrc",
            "_vimrc",
            "_exrc",
            "_gvimrc",
            "vimrc",
            "gvimrc",
        ],
        mime_types: &["text/x-vim"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"^([ \t:]*)(py(?:t(?:h(?:o(?:n)?)?)?)?)([ \t]*)(<<)([ \t]*)(.*)((?:\n|.)*)(\6)").bygroups(&[E::UsingSelf("root"), E::Token(T::Keyword), E::Token(T::Text), E::Token(T::Operator), E::Token(T::Text), E::Token(T::Text), E::Using("Python"), E::Token(T::Text)]),
            rule(r"^([ \t:]*)(py(?:t(?:h(?:o(?:n)?)?)?)?)([ \t])(.*)").bygroups(&[E::UsingSelf("root"), E::Token(T::Keyword), E::Token(T::Text), E::Using("Python")]),
            rule(r#"^\s*".*"#).token(T::Comment),
            rule(r"[ \t]+").token(T::Text),
            rule(r"/(\\\\|\\/|[^\n/])*/").token(T::LiteralStringRegex),
            rule(r#""(\\\\|\\"|[^\n"])*""#).token(T::LiteralStringDouble),
            rule(r"'(''|[^\n'])*'").token(T::LiteralStringSingle),
            rule(r#"(?<=\s)"[^\-:.%#=*].*"#).token(T::Comment),
            rule(r"-?\d+").token(T::LiteralNumber),
            rule(r"#[0-9a-f]{6}").token(T::LiteralNumberHex),
            rule(r"^:").token(T::Punctuation),
            rule(r"[()<>+=!|,~-]").token(T::Punctuation),
            rule(r"\b(let|if|else|endif|elseif|fun|function|endfunction|set|map|autocmd|filetype|hi(ghlight)?|execute|syntax|colorscheme)\b").token(T::Keyword),
            rule(r"\b(NONE|bold|italic|underline|dark|light)\b").token(T::NameBuiltin),
            rule(r"\b\w+\b").token(T::NameOther),
            rule(r"\n").token(T::Text),
            rule(r".").token(T::Text),
        ]),
    ],
};
