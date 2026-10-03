//! Chroma's `restructuredtext.xml` lexer, converted to Rust
//! (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "restructuredtext",
    config: ConfigDef {
        name: "reStructuredText",
        aliases: &["rst", "rest", "restructuredtext"],
        filenames: &["*.rst", "*.rest"],
        mime_types: &["text/x-rst", "text/prs.fallenstein.rst"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("inline", &[
            rule(r"\\.").token(T::Text),
            rule(r"``").token(T::LiteralString).push(&["literal"]),
            rule(r"(`.+?)(<.+?>)(`__?)").groups(&[T::LiteralString, T::LiteralStringInterpol, T::LiteralString]),
            rule(r"`.+?`__?").token(T::LiteralString),
            rule(r"(`.+?`)(:[a-zA-Z0-9:-]+?:)?").groups(&[T::NameVariable, T::NameAttribute]),
            rule(r"(:[a-zA-Z0-9:-]+?:)(`.+?`)").groups(&[T::NameAttribute, T::NameVariable]),
            rule(r"\*\*.+?\*\*").token(T::GenericStrong),
            rule(r"\*.+?\*").token(T::GenericEmph),
            rule(r"\[.*?\]_").token(T::LiteralString),
            rule(r"<.+?>").token(T::NameTag),
            rule(r"[^\\\n\[*`:]+").token(T::Text),
            rule(r".").token(T::Text),
        ]),
        ("literal", &[
            rule(r"[^`]+").token(T::LiteralString),
            rule(r#"``((?=$)|(?=[-/:.,; \n\x00\‐\‑\‒\–\—\ \'\"\)\]\}\>\’\”\»\!\?]))"#).token(T::LiteralString).pop(1),
            rule(r"`").token(T::LiteralString),
        ]),
        ("root", &[
            rule(r#"^(=+|-+|`+|:+|\.+|\'+|"+|~+|\^+|_+|\*+|\++|#+)([ \t]*\n)(.+)(\n)(\1)(\n)"#).groups(&[T::GenericHeading, T::Text, T::GenericHeading, T::Text, T::GenericHeading, T::Text]),
            rule(r#"^(\S.*)(\n)(={3,}|-{3,}|`{3,}|:{3,}|\.{3,}|\'{3,}|"{3,}|~{3,}|\^{3,}|_{3,}|\*{3,}|\+{3,}|#{3,})(\n)"#).groups(&[T::GenericHeading, T::Text, T::GenericHeading, T::Text]),
            rule(r"^(\s*)([-*+])( .+\n(?:\1  .+\n)*)").bygroups(&[E::Token(T::Text), E::Token(T::LiteralNumber), E::UsingSelf("inline")]),
            rule(r"^(\s*)([0-9#ivxlcmIVXLCM]+\.)( .+\n(?:\1  .+\n)*)").bygroups(&[E::Token(T::Text), E::Token(T::LiteralNumber), E::UsingSelf("inline")]),
            rule(r"^(\s*)(\(?[0-9#ivxlcmIVXLCM]+\))( .+\n(?:\1  .+\n)*)").bygroups(&[E::Token(T::Text), E::Token(T::LiteralNumber), E::UsingSelf("inline")]),
            rule(r"^(\s*)([A-Z]+\.)( .+\n(?:\1  .+\n)+)").bygroups(&[E::Token(T::Text), E::Token(T::LiteralNumber), E::UsingSelf("inline")]),
            rule(r"^(\s*)(\(?[A-Za-z]+\))( .+\n(?:\1  .+\n)+)").bygroups(&[E::Token(T::Text), E::Token(T::LiteralNumber), E::UsingSelf("inline")]),
            rule(r"^(\s*)(\|)( .+\n(?:\|  .+\n)*)").bygroups(&[E::Token(T::Text), E::Token(T::Operator), E::UsingSelf("inline")]),
            rule(r"^( *\.\.)(\s*)((?:source)?code(?:-block)?)(::)([ \t]*)([^\n]+)(\n[ \t]*\n)([ \t]+)(.*)(\n)((?:(?:\8.*|)\n)+)").emit_func("rstCodeBlock"),
            rule(r"^( *\.\.)(\s*)([\w:-]+?)(::)(?:([ \t]*)(.*))").bygroups(&[E::Token(T::Punctuation), E::Token(T::Text), E::Token(T::OperatorWord), E::Token(T::Punctuation), E::Token(T::Text), E::UsingSelf("inline")]),
            rule(r"^( *\.\.)(\s*)(_(?:[^:\\]|\\.)+:)(.*?)$").bygroups(&[E::Token(T::Punctuation), E::Token(T::Text), E::Token(T::NameTag), E::UsingSelf("inline")]),
            rule(r"^( *\.\.)(\s*)(\[.+\])(.*?)$").bygroups(&[E::Token(T::Punctuation), E::Token(T::Text), E::Token(T::NameTag), E::UsingSelf("inline")]),
            rule(r"^( *\.\.)(\s*)(\|.+\|)(\s*)([\w:-]+?)(::)(?:([ \t]*)(.*))").bygroups(&[E::Token(T::Punctuation), E::Token(T::Text), E::Token(T::NameTag), E::Token(T::Text), E::Token(T::OperatorWord), E::Token(T::Punctuation), E::Token(T::Text), E::UsingSelf("inline")]),
            rule(r"^ *\.\..*(\n( +.*\n|\n)+)?").token(T::CommentPreproc),
            rule(r"^( *)(:[a-zA-Z-]+:)(\s*)$").groups(&[T::Text, T::NameClass, T::Text]),
            rule(r"^( *)(:.*?:)([ \t]+)(.*?)$").groups(&[T::Text, T::NameClass, T::Text, T::NameFunction]),
            rule(r"^(\S.*(?<!::)\n)((?:(?: +.*)\n)+)").bygroups(&[E::UsingSelf("inline"), E::UsingSelf("inline")]),
            rule(r"(::)(\n[ \t]*\n)([ \t]+)(.*)(\n)((?:(?:\3.*|)\n)+)").groups(&[T::LiteralStringEscape, T::Text, T::LiteralString, T::LiteralString, T::Text, T::LiteralString]),
            include("inline"),
        ]),
    ],
};
