//! Chroma's `bqn.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "bqn",
    config: ConfigDef {
        name: "BQN",
        aliases: &["bqn"],
        filenames: &["*.bqn"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"\A#!.+$").token(T::CommentPreproc),
            rule(r"#.*$").token(T::CommentSingle),
            rule(r#""(?:[^"]|"")*""#).token(T::LiteralString),
            rule(r"[{}]").token(T::KeywordPseudo),
            rule(r"[⟨⟩\[\]‿]").token(T::KeywordPseudo),
            rule(r"[()]").token(T::Punctuation),
            rule(r"[:;?]").token(T::Punctuation),
            rule(r"[⋄,]").token(T::KeywordPseudo),
            rule(r"[←⇐↩→]").token(T::Text),
            rule(r"'.'").token(T::LiteralStringChar),
            rule(r"[˙˜˘¨⌜⁼´˝`]").token(T::Operator),
            rule(r"[∘○⊸⟜⌾⊘◶⎉⚇⍟⎊]").token(T::OperatorWord),
            rule(r"[𝔽𝔾𝕎𝕏𝕊+\-×÷⋆√⌊⌈|¬∧∨<>≠=≤≥≡≢⊣⊢⥊∾≍⋈↑↓↕«»⌽⍉/⍋⍒⊏⊑⊐⊒∊⍷⊔!⍕⍎]").token(T::NameFunction),
            rule(r"[𝕗𝕘𝕨𝕩𝕤]").token(T::Name),
            rule(r"·").token(T::NameConstant),
            rule(r"@").token(T::LiteralStringChar),
            rule(r"\d+(?:\.\d+)?[eE]¯?\d+").token(T::LiteralNumber),
            rule(r"[¯∞π]?(?:\d*\.?\b\d+(?:e[+¯]?\d+|E[+¯]?\d+)?|¯|∞|π)(?:j¯?(?:(?:\d+(?:\.\d+)?|\.\d+)(?:e[+¯]?\d+|E[+¯]?\d+)?|¯|∞|π))?").token(T::LiteralNumber),
            rule(r"(•?[a-z][A-Z_a-z0-9π∞¯]*|𝕣)").token(T::Name),
            rule(r"•?[A-Z][A-Z_a-z0-9π∞¯]*").token(T::NameFunction),
            rule(r"(•?_[A-Za-z][A-Z_a-z0-9π∞¯]*|_𝕣)").token(T::Operator),
            rule(r"(•?_[A-Za-z][A-Z_a-z0-9π∞¯]*_|_𝕣_)").token(T::OperatorWord),
            rule(r"\.").token(T::Text),
        ]),
    ],
};
