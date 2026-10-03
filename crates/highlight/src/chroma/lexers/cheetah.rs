//! Chroma's `cheetah.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "cheetah",
    config: ConfigDef {
        name: "Cheetah",
        aliases: &["cheetah", "spitfire"],
        filenames: &["*.tmpl", "*.spt"],
        mime_types: &["application/x-cheetah", "application/x-spitfire"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"(##[^\n]*)$").groups(&[T::Comment]),
            rule(r"#[*](.|\n)*?[*]#").token(T::Comment),
            rule(r"#end[^#\n]*(?:#|$)").token(T::CommentPreproc),
            rule(r"#slurp$").token(T::CommentPreproc),
            rule(r"(#[a-zA-Z]+)([^#\n]*)(#|$)").bygroups(&[E::Token(T::CommentPreproc), E::Using("Python"), E::Token(T::CommentPreproc)]),
            rule(r"(\$)([a-zA-Z_][\w.]*\w)").bygroups(&[E::Token(T::CommentPreproc), E::Using("Python")]),
            rule(r"(\$\{!?)(.*?)(\})(?s)").bygroups(&[E::Token(T::CommentPreproc), E::Using("Python"), E::Token(T::CommentPreproc)]),
            rule(r"(?sx)
                (.+?)               # anything, followed by:
                (?:
                 (?=\#[#a-zA-Z]*) | # an eval comment
                 (?=\$[a-zA-Z_{]) | # a substitution
                 \Z                 # end of string
                )
            ").token(T::Other),
            rule(r"\s+").token(T::Text),
        ]),
    ],
};
