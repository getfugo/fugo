//! Chroma's `genshi_html.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "genshi_html",
    config: ConfigDef {
        name: "Genshi HTML",
        aliases: &["html+genshi", "html+kid"],
        mime_types: &["text/html+genshi"],
        dot_all: true,
        not_multiline: true,
        ..ConfigDef::EMPTY
    },
    states: &[
        ("attr", &[
            rule(r#"""#).token(T::LiteralString).push(&["attr-dstring"]),
            rule(r"'").token(T::LiteralString).push(&["attr-sstring"]),
            rule(r"[^\s>]*").token(T::LiteralString).pop(1),
        ]),
        ("attr-dstring", &[
            rule(r#"""#).token(T::LiteralString).pop(1),
            include("strings"),
            rule(r"'").token(T::LiteralString),
        ]),
        ("attr-sstring", &[
            rule(r"'").token(T::LiteralString).pop(1),
            include("strings"),
            rule(r"'").token(T::LiteralString),
        ]),
        ("pyattr", &[
            rule(r#"(")(.*?)(")"#).bygroups(&[E::Token(T::LiteralString), E::Using("Python"), E::Token(T::LiteralString)]).pop(1),
            rule(r"(')(.*?)(')").bygroups(&[E::Token(T::LiteralString), E::Using("Python"), E::Token(T::LiteralString)]).pop(1),
            rule(r"[^\s>]+").token(T::LiteralString).pop(1),
        ]),
        ("pytag", &[
            rule(r"\s+").token(T::Text),
            rule(r"[\w:-]+\s*=").token(T::NameAttribute).push(&["pyattr"]),
            rule(r"/?\s*>").token(T::NameTag).pop(1),
        ]),
        ("root", &[
            rule(r"[^<$]+").token(T::Other),
            rule(r"(<\?python)(.*?)(\?>)").bygroups(&[E::Token(T::CommentPreproc), E::Using("Python"), E::Token(T::CommentPreproc)]),
            rule(r"<\s*(script|style)\s*.*?>.*?<\s*/\1\s*>").token(T::Other),
            rule(r"<\s*py:[a-zA-Z0-9]+").token(T::NameTag).push(&["pytag"]),
            rule(r"<\s*[a-zA-Z0-9:.]+").token(T::NameTag).push(&["tag"]),
            include("variable"),
            rule(r"[<$]").token(T::Other),
        ]),
        ("strings", &[
            rule(r#"[^"'$]+"#).token(T::LiteralString),
            include("variable"),
        ]),
        ("tag", &[
            rule(r"\s+").token(T::Text),
            rule(r"py:[\w-]+\s*=").token(T::NameAttribute).push(&["pyattr"]),
            rule(r"[\w:-]+\s*=").token(T::NameAttribute).push(&["attr"]),
            rule(r"/?\s*>").token(T::NameTag).pop(1),
        ]),
        ("variable", &[
            rule(r"(?<!\$)(\$\{)(.+?)(\})").bygroups(&[E::Token(T::CommentPreproc), E::Using("Python"), E::Token(T::CommentPreproc)]),
            rule(r"(?<!\$)(\$)([a-zA-Z_][\w\.]*)").token(T::NameVariable),
        ]),
    ],
};
