//! Chroma's `markdown.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "markdown",
    config: ConfigDef {
        name: "markdown",
        aliases: &["md", "mkd"],
        filenames: &["*.md", "*.mkd", "*.markdown"],
        mime_types: &["text/x-markdown"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("inline", &[
            rule(r"\\.").token(T::Text),
            rule(r"(\s)(\*|_)((?:(?!\2).)*)(\2)((?=\W|\n))").groups(&[T::Text, T::GenericEmph, T::GenericEmph, T::GenericEmph, T::Text]),
            rule(r"(\s)((\*\*|__).*?)\3((?=\W|\n))").groups(&[T::Text, T::GenericStrong, T::GenericStrong, T::Text]),
            rule(r"(\s)(~~[^~]+~~)((?=\W|\n))").groups(&[T::Text, T::GenericDeleted, T::Text]),
            rule(r"`[^`]+`").token(T::LiteralStringBacktick),
            rule(r"[@#][\w/:]+").token(T::NameEntity),
            rule(r"(!?\[)([^]]+)(\])(\()([^)]+)(\))").groups(&[T::Text, T::NameTag, T::Text, T::Text, T::NameAttribute, T::Text]),
            rule(r".|\n").token(T::Other),
        ]),
        ("root", &[
            rule(r"^(#[^#].+\n)").groups(&[T::GenericHeading]),
            rule(r"^(#{2,6}.+\n)").groups(&[T::GenericSubheading]),
            rule(r"^(\s*)([*-] )(\[[ xX]\])( .+\n)").bygroups(&[E::Token(T::Text), E::Token(T::Keyword), E::Token(T::Keyword), E::UsingSelf("inline")]),
            rule(r"^(\s*)([*-])(\s)(.+\n)").bygroups(&[E::Token(T::Text), E::Token(T::Keyword), E::Token(T::Text), E::UsingSelf("inline")]),
            rule(r"^(\s*)([0-9]+\.)( .+\n)").bygroups(&[E::Token(T::Text), E::Token(T::Keyword), E::UsingSelf("inline")]),
            rule(r"^(\s*>\s)(.+\n)").groups(&[T::Keyword, T::GenericEmph]),
            rule(r"^(```\n)([\w\W]*?)(^```$)").groups(&[T::LiteralString, T::Text, T::LiteralString]),
            rule(r"^(```)(\w+)(\n)([\w\W]*?)(^```$)").using_by_group(2, 4, &[E::Token(T::LiteralString), E::Token(T::LiteralString), E::Token(T::LiteralString), E::Token(T::Text), E::Token(T::LiteralString)]),
            include("inline"),
        ]),
    ],
};
