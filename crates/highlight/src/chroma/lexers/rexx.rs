//! Chroma's `rexx.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "rexx",
    config: ConfigDef {
        name: "Rexx",
        aliases: &["rexx", "arexx"],
        filenames: &["*.rexx", "*.rex", "*.rx", "*.arexx"],
        mime_types: &["text/x-rexx"],
        case_insensitive: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("keyword", &[
            rule(r"(address|arg|by|call|do|drop|else|end|exit|for|forever|if|interpret|iterate|leave|nop|numeric|off|on|options|parse|pull|push|queue|return|say|select|signal|to|then|trace|until|while)\b").token(T::KeywordReserved),
        ]),
        ("operator", &[
            rule(r"(-|//|/|\(|\)|\*\*|\*|\\<<|\\<|\\==|\\=|\\>>|\\>|\\|\|\||\||&&|&|%|\+|<<=|<<|<=|<>|<|==|=|><|>=|>>=|>>|>|¬<<|¬<|¬==|¬=|¬>>|¬>|¬|\.|,)").token(T::Operator),
        ]),
        ("string_double", &[
            rule(r#"[^"\n]+"#).token(T::LiteralString),
            rule(r#""""#).token(T::LiteralString),
            rule(r#"""#).token(T::LiteralString).pop(1),
            rule(r"\n").token(T::Text).pop(1),
        ]),
        ("string_single", &[
            rule(r"[^\'\n]").token(T::LiteralString),
            rule(r"\'\'").token(T::LiteralString),
            rule(r"\'").token(T::LiteralString).pop(1),
            rule(r"\n").token(T::Text).pop(1),
        ]),
        ("comment", &[
            rule(r"[^*]+").token(T::CommentMultiline),
            rule(r"\*/").token(T::CommentMultiline).pop(1),
            rule(r"\*").token(T::CommentMultiline),
        ]),
        ("root", &[
            rule(r"\s").token(T::TextWhitespace),
            rule(r"/\*").token(T::CommentMultiline).push(&["comment"]),
            rule(r#"""#).token(T::LiteralString).push(&["string_double"]),
            rule(r"'").token(T::LiteralString).push(&["string_single"]),
            rule(r"[0-9]+(\.[0-9]+)?(e[+-]?[0-9])?").token(T::LiteralNumber),
            rule(r"([a-z_]\w*)(\s*)(:)(\s*)(procedure)\b").groups(&[T::NameFunction, T::TextWhitespace, T::Operator, T::TextWhitespace, T::KeywordDeclaration]),
            rule(r"([a-z_]\w*)(\s*)(:)").groups(&[T::NameLabel, T::TextWhitespace, T::Operator]),
            include("function"),
            include("keyword"),
            include("operator"),
            rule(r"[a-z_]\w*").token(T::Text),
        ]),
        ("function", &[
            rule(r"(sourceline|wordlength|errortext|translate|wordindex|condition|datatype|subword|lineout|lastpos|delword|address|charout|wordpos|compare|overlay|reverse|symbol|stream|charin|center|delstr|verify|digits|abbrev|bitxor|format|random|insert|bitand|queued|length|linein|substr|copies|xrange|space|words|lines|bitor|trunc|strip|right|value|chars|trace|sign|form|fuzz|word|left|time|date|c2d|d2c|d2x|c2x|pos|b2x|arg|abs|min|x2b|x2c|x2d|max)(\s*)(\()").groups(&[T::NameBuiltin, T::TextWhitespace, T::Operator]),
        ]),
    ],
};
