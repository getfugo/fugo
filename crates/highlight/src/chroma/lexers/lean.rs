//! Chroma's `lean.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "lean",
    config: ConfigDef {
        name: "Lean4",
        aliases: &["lean4", "lean"],
        filenames: &["*.lean"],
        mime_types: &["text/x-lean4", "text/x-lean"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("expression", &[
            rule(r"\s+").token(T::TextWhitespace),
            rule(r"/--").token(T::LiteralStringDoc).push(&["docstring"]),
            rule(r"/-").token(T::Comment).push(&["comment"]),
            rule(r"--.*$").token(T::CommentSingle),
            rule(r"\b(Type|Prop|Sort)\b").token(T::KeywordType),
            rule(r"\b(sorry|admit)\b").token(T::GenericError),
            rule(r"(!=|\#|\&|\&\&|\*|\+|\-|/|@|!|\-\.|\->|\.|\.\.|\.\.\.|::|:>|;|;;|<|<\-|=|==|>|_|\||\|\||\~|=>|<=|>=|/\\|\\/|∀|Π|λ|↔|∧|∨|≠|≤|≥|¬|⁻¹|⬝|▸|→|∃|≈|×|⌞|⌟|≡|⟨|⟩|↦)").token(T::NameBuiltinPseudo),
            rule(r"(\(|\)|:|\{|\}|\[|\]|⦃|⦄|:=|,)").token(T::Operator),
            rule(r"(?![λΠΣ])[_a-zA-Zα-ωΑ-Ωϊ-ϻἀ-῾℀-⅏𝒜-𝖟](?:(?![λΠΣ])[_a-zA-Zα-ωΑ-Ωϊ-ϻἀ-῾℀-⅏𝒜-𝖟0-9'ⁿ-₉ₐ-ₜᵢ-ᵪ!?])*").token(T::Name),
            rule(r"``?(?![λΠΣ])[_a-zA-Zα-ωΑ-Ωϊ-ϻἀ-῾℀-⅏𝒜-𝖟](?:(?![λΠΣ])[_a-zA-Zα-ωΑ-Ωϊ-ϻἀ-῾℀-⅏𝒜-𝖟0-9'ⁿ-₉ₐ-ₜᵢ-ᵪ!?])*(\.(?![λΠΣ])[_a-zA-Zα-ωΑ-Ωϊ-ϻἀ-῾℀-⅏𝒜-𝖟](?:(?![λΠΣ])[_a-zA-Zα-ωΑ-Ωϊ-ϻἀ-῾℀-⅏𝒜-𝖟0-9'ⁿ-₉ₐ-ₜᵢ-ᵪ!?])*)*").token(T::LiteralStringSymbol),
            rule(r"(?<=\.)\d+").token(T::LiteralNumber),
            rule(r"(\d+\.\d*)([eE][+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"\d+").token(T::LiteralNumberInteger),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"[~?][a-z][\w\']*:").token(T::NameVariable),
            rule(r"\S").token(T::NameBuiltinPseudo),
        ]),
        ("root", &[
            rule(r"\b(import|unif_hint|renaming|inline|hiding|lemma|variable|theorem|axiom|inductive|structure|universe|alias|\#help|precedence|postfix|prefix|infix|infixl|infixr|notation|\#eval|\#check|\#reduce|\#exit|end|private|using|namespace|instance|section|protected|export|set_option|extends|open|example|\#print|opaque|def|macro|elab|syntax|macro_rules|\#reduce|where|abbrev|noncomputable|class|attribute|\#synth|mutual|scoped|local)\b").token(T::KeywordNamespace),
            rule(r"\b(forall|fun|obtain|from|have|show|assume|let|if|else|then|by|in|with|calc|match|nomatch|do|at)\b").token(T::Keyword),
            rule(r"@\[").token(T::KeywordDeclaration).push(&["attribute"]),
            include("expression"),
        ]),
        ("attribute", &[
            rule(r"\]").token(T::KeywordDeclaration).pop(1),
            include("expression"),
        ]),
        ("comment", &[
            rule(r"[^/-]+").token(T::CommentMultiline),
            rule(r"/-").token(T::CommentMultiline).push(&[]),
            rule(r"-/").token(T::CommentMultiline).pop(1),
            rule(r"[/-]").token(T::CommentMultiline),
        ]),
        ("docstring", &[
            rule(r"[^/-]+").token(T::LiteralStringDoc),
            rule(r"-/").token(T::LiteralStringDoc).pop(1),
            rule(r"[/-]").token(T::LiteralStringDoc),
        ]),
        ("string", &[
            rule(r#"[^\\"]+"#).token(T::LiteralStringDouble),
            rule(r#"\\[n"\\\n]"#).token(T::LiteralStringEscape),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
    ],
};
