//! Chroma's `jungle.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "jungle",
    config: ConfigDef {
        name: "Jungle",
        aliases: &["jungle"],
        filenames: &["*.jungle"],
        mime_types: &["text/x-jungle"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("var", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\b(((re)?source|barrel)Path|excludeAnnotations|annotations|lang)\b").token(T::NameBuiltin),
            rule(r"\bbase\b").token(T::NameConstant),
            rule(r"\b(ind|zsm|hrv|ces|dan|dut|eng|fin|fre|deu|gre|hun|ita|nob|po[lr]|rus|sl[ov]|spa|swe|ara|heb|zh[st]|jpn|kor|tha|vie|bul|tur)").token(T::NameConstant),
            rule(r"\b((semi)?round|rectangle)(-\d+x\d+)?\b").token(T::NameConstant),
            rule(r"[\.;\[\]\(\$]").token(T::Punctuation),
            rule(r"\)").token(T::Punctuation).pop(1),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
            rule("").pop(1),
        ]),
        ("root", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\n").token(T::Text),
            rule(r"#(\n|[\w\W]*?[^#]\n)").token(T::CommentSingle),
            rule(r"^(?=\S)").token(T::NoHighlight).push(&["instruction"]),
            rule(r"[\.;\[\]\(\)\$]").token(T::Punctuation),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("instruction", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"=").token(T::Operator).push(&["value"]),
            rule(r"(?=\S)").token(T::NoHighlight).push(&["var"]),
            rule("").pop(1),
        ]),
        ("value", &[
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"\$\(").token(T::Punctuation).push(&["var"]),
            rule(r"[;\[\]\(\)\$]").token(T::Punctuation),
            rule(r"#(\n|[\w\W]*?[^#]\n)").token(T::CommentSingle),
            rule(r"[\w_\-\.\/\\]+").token(T::Text),
            rule("").pop(1),
        ]),
    ],
};
