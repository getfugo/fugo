//! Chroma's `apl.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "apl",
    config: ConfigDef {
        name: "APL",
        aliases: &["apl"],
        filenames: &["*.apl"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"[⍝#].*$").token(T::CommentSingle),
            rule(r"\'((\'\')|[^\'])*\'").token(T::LiteralStringSingle),
            rule(r#""(("")|[^"])*""#).token(T::LiteralStringDouble),
            rule(r"[⋄◇()]").token(T::Punctuation),
            rule(r"[\[\];]").token(T::LiteralStringRegex),
            rule(r"⎕[A-Za-zΔ∆⍙][A-Za-zΔ∆⍙_¯0-9]*").token(T::NameFunction),
            rule(r"[A-Za-zΔ∆⍙_][A-Za-zΔ∆⍙_¯0-9]*").token(T::NameVariable),
            rule(r"¯?(0[Xx][0-9A-Fa-f]+|[0-9]*\.?[0-9]+([Ee][+¯]?[0-9]+)?|¯|∞)([Jj]¯?(0[Xx][0-9A-Fa-f]+|[0-9]*\.?[0-9]+([Ee][+¯]?[0-9]+)?|¯|∞))?").token(T::LiteralNumber),
            rule(r"[\.\\/⌿⍀¨⍣⍨⍠⍤∘⍥@⌺⌶⍢]").token(T::NameAttribute),
            rule(r"[+\-×÷⌈⌊∣|⍳?*⍟○!⌹<≤=>≥≠≡≢∊⍷∪∩~∨∧⍱⍲⍴,⍪⌽⊖⍉↑↓⊂⊃⌷⍋⍒⊤⊥⍕⍎⊣⊢⍁⍂≈⌸⍯↗⊆⍸]").token(T::Operator),
            rule(r"⍬").token(T::NameConstant),
            rule(r"[⎕⍞]").token(T::NameVariableGlobal),
            rule(r"[←→]").token(T::KeywordDeclaration),
            rule(r"[⍺⍵⍶⍹∇:]").token(T::NameBuiltinPseudo),
            rule(r"[{}]").token(T::KeywordType),
        ]),
    ],
};
