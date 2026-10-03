//! Chroma's `mako.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "mako",
    config: ConfigDef {
        name: "Mako",
        aliases: &["mako"],
        filenames: &["*.mao"],
        mime_types: &["application/x-mako"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(\s*)(%)(\s*end(?:\w+))(\n|\Z)").groups(&[T::Text, T::CommentPreproc, T::Keyword, T::Other]),
            rule(r"(\s*)(%)([^\n]*)(\n|\Z)").bygroups(&[E::Token(T::Text), E::Token(T::CommentPreproc), E::Using("Python"), E::Token(T::Other)]),
            rule(r"(\s*)(##[^\n]*)(\n|\Z)").groups(&[T::Text, T::CommentPreproc, T::Other]),
            rule(r"(?s)<%doc>.*?</%doc>").token(T::CommentPreproc),
            rule(r"(<%)([\w.:]+)").groups(&[T::CommentPreproc, T::NameBuiltin]).push(&["tag"]),
            rule(r"(</%)([\w.:]+)(>)").groups(&[T::CommentPreproc, T::NameBuiltin, T::CommentPreproc]),
            rule(r"<%(?=([\w.:]+))").token(T::CommentPreproc).push(&["ondeftags"]),
            rule(r"(<%(?:!?))(.*?)(%>)(?s)").bygroups(&[E::Token(T::CommentPreproc), E::Using("Python"), E::Token(T::CommentPreproc)]),
            rule(r"(\$\{)(.*?)(\})").bygroups(&[E::Token(T::CommentPreproc), E::Using("Python"), E::Token(T::CommentPreproc)]),
            rule(r"(?sx)
                (.+?)                # anything, followed by:
                (?:
                 (?<=\n)(?=%|\#\#) | # an eval or comment line
                 (?=\#\*) |          # multiline comment
                 (?=</?%) |          # a python block
                                     # call start or end
                 (?=\$\{) |          # a substitution
                 (?<=\n)(?=\s*%) |
                                     # - don't consume
                 (\\\n) |            # an escaped newline
                 \Z                  # end of string
                )
            ").groups(&[T::Other, T::Operator]),
            rule(r"\s+").token(T::Text),
        ]),
        ("ondeftags", &[
            rule(r"<%").token(T::CommentPreproc),
            rule(r"(?<=<%)(include|inherit|namespace|page)").token(T::NameBuiltin),
            include("tag"),
        ]),
        ("tag", &[
            rule(r#"((?:\w+)\s*=)(\s*)(".*?")"#).groups(&[T::NameAttribute, T::Text, T::LiteralString]),
            rule(r"/?\s*>").token(T::CommentPreproc).pop(1),
            rule(r"\s+").token(T::Text),
        ]),
        ("attr", &[
            rule(r#"".*?""#).token(T::LiteralString).pop(1),
            rule(r"'.*?'").token(T::LiteralString).pop(1),
            rule(r"[^\s>]+").token(T::LiteralString).pop(1),
        ]),
    ],
};
