//! Chroma's `myghty.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "myghty",
    config: ConfigDef {
        name: "Myghty",
        aliases: &["myghty"],
        filenames: &["*.myt", "autodelegate"],
        mime_types: &["application/x-myghty"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"(<%(?:def|method))(\s*)(.*?)(>)(.*?)(</%\2\s*>)(?s)").bygroups(&[E::Token(T::NameTag), E::Token(T::Text), E::Token(T::NameFunction), E::Token(T::NameTag), E::UsingSelf("root"), E::Token(T::NameTag)]),
            rule(r"(<%\w+)(.*?)(>)(.*?)(</%\2\s*>)(?s)").bygroups(&[E::Token(T::NameTag), E::Token(T::NameFunction), E::Token(T::NameTag), E::Using("Python2"), E::Token(T::NameTag)]),
            rule(r"(<&[^|])(.*?)(,.*?)?(&>)").bygroups(&[E::Token(T::NameTag), E::Token(T::NameFunction), E::Using("Python2"), E::Token(T::NameTag)]),
            rule(r"(<&\|)(.*?)(,.*?)?(&>)(?s)").bygroups(&[E::Token(T::NameTag), E::Token(T::NameFunction), E::Using("Python2"), E::Token(T::NameTag)]),
            rule(r"</&>").token(T::NameTag),
            rule(r"(<%!?)(.*?)(%>)(?s)").bygroups(&[E::Token(T::NameTag), E::Using("Python2"), E::Token(T::NameTag)]),
            rule(r"(?<=^)#[^\n]*(\n|\Z)").token(T::Comment),
            rule(r"(?<=^)(%)([^\n]*)(\n|\Z)").bygroups(&[E::Token(T::NameTag), E::Using("Python2"), E::Token(T::Other)]),
            rule(r"(?sx)
                 (.+?)               # anything, followed by:
                 (?:
                  (?<=\n)(?=[%#]) |  # an eval or comment line
                  (?=</?[%&]) |      # a substitution or block or
                                     # call start or end
                                     # - don't consume
                  (\\\n) |           # an escaped newline
                  \Z                 # end of string
                 )").groups(&[T::Other, T::Operator]),
        ]),
    ],
};
