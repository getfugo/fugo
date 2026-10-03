//! Chroma's `cap_n_proto.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "cap_n_proto",
    config: ConfigDef {
        name: "Cap'n Proto",
        aliases: &["capnp"],
        filenames: &["*.capnp"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"#.*?$").token(T::CommentSingle),
            rule(r"@[0-9a-zA-Z]*").token(T::NameDecorator),
            rule(r"=").token(T::Literal).push(&["expression"]),
            rule(r":").token(T::NameClass).push(&["type"]),
            rule(r"\$").token(T::NameAttribute).push(&["annotation"]),
            rule(r"(struct|enum|interface|union|import|using|const|annotation|extends|in|of|on|as|with|from|fixed)\b").token(T::Keyword),
            rule(r"[\w.]+").token(T::Name),
            rule(r"[^#@=:$\w]+").token(T::Text),
        ]),
        ("type", &[
            rule(r"[^][=;,(){}$]+").token(T::NameClass),
            rule(r"[[(]").token(T::NameClass).push(&["parentype"]),
            rule("").pop(1),
        ]),
        ("parentype", &[
            rule(r"[^][;()]+").token(T::NameClass),
            rule(r"[[(]").token(T::NameClass).push(&[]),
            rule(r"[])]").token(T::NameClass).pop(1),
            rule("").pop(1),
        ]),
        ("expression", &[
            rule(r"[^][;,(){}$]+").token(T::Literal),
            rule(r"[[(]").token(T::Literal).push(&["parenexp"]),
            rule("").pop(1),
        ]),
        ("parenexp", &[
            rule(r"[^][;()]+").token(T::Literal),
            rule(r"[[(]").token(T::Literal).push(&[]),
            rule(r"[])]").token(T::Literal).pop(1),
            rule("").pop(1),
        ]),
        ("annotation", &[
            rule(r"[^][;,(){}=:]+").token(T::NameAttribute),
            rule(r"[[(]").token(T::NameAttribute).push(&["annexp"]),
            rule("").pop(1),
        ]),
        ("annexp", &[
            rule(r"[^][;()]+").token(T::NameAttribute),
            rule(r"[[(]").token(T::NameAttribute).push(&[]),
            rule(r"[])]").token(T::NameAttribute).pop(1),
            rule("").pop(1),
        ]),
    ],
};
