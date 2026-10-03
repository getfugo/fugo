//! Chroma's `coq.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "coq",
    config: ConfigDef {
        name: "Coq",
        aliases: &["coq"],
        filenames: &["*.v"],
        mime_types: &["text/x-coq"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("string", &[
            rule(r#"[^"]+"#).token(T::LiteralStringDouble),
            rule(r#""""#).token(T::LiteralStringDouble),
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
        ]),
        ("dotted", &[
            rule(r"\s+").token(T::Text),
            rule(r"\.").token(T::Punctuation),
            rule(r"[A-Z][\w\']*(?=\s*\.)").token(T::NameNamespace),
            rule(r"[A-Z][\w\']*").token(T::NameClass).pop(1),
            rule(r"[a-z][a-z0-9_\']*").token(T::Name).pop(1),
            rule("").pop(1),
        ]),
        ("root", &[
            rule(r"\s+").token(T::Text),
            rule(r"false|true|\(\)|\[\]").token(T::NameBuiltinPseudo),
            rule(r"\(\*").token(T::Comment).push(&["comment"]),
            rule(r"\b(Projections|Monomorphic|Polymorphic|Proposition|CoInductive|Hypothesis|CoFixpoint|Contextual|Definition|Parameters|Hypotheses|Structure|Inductive|Corollary|Implicits|Parameter|Variables|Arguments|Canonical|Printing|Coercion|Reserved|Universe|Notation|Instance|Fixpoint|Variable|Morphism|Relation|Existing|Implicit|Example|Theorem|Delimit|Defined|Rewrite|outside|Require|Resolve|Section|Context|Prenex|Strict|Module|Import|Export|Global|inside|Remark|Tactic|Search|Record|Scope|Unset|Check|Local|Close|Class|Graph|Proof|Lemma|Print|Axiom|Show|Goal|Open|Fact|Hint|Bind|Ltac|Save|View|Let|Set|All|End|Qed)\b").token(T::KeywordNamespace),
            rule(r"\b(exists2|nosimpl|struct|exists|return|forall|match|cofix|then|with|else|for|fix|let|fun|end|is|of|if|in|as)\b").token(T::Keyword),
            rule(r"\b(Type|Prop)\b").token(T::KeywordType),
            rule(r"\b(native_compute|setoid_rewrite|etransitivity|econstructor|transitivity|autorewrite|constructor|cutrewrite|vm_compute|bool_congr|generalize|inversion|induction|injection|nat_congr|intuition|destruct|suffices|erewrite|symmetry|nat_norm|replace|rewrite|compute|pattern|trivial|without|assert|unfold|change|eapply|intros|unlock|revert|rename|refine|eauto|tauto|after|right|congr|split|field|simpl|intro|clear|apply|using|subst|case|left|suff|loss|wlog|have|fold|ring|move|lazy|elim|pose|auto|red|cbv|hnf|cut|set)\b").token(T::Keyword),
            rule(r"\b(contradiction|discriminate|reflexivity|assumption|congruence|romega|omega|exact|solve|tauto|done|by)\b").token(T::KeywordPseudo),
            rule(r"\b(repeat|first|idtac|last|try|do)\b").token(T::KeywordReserved),
            rule(r"\b([A-Z][\w\']*)").token(T::Name),
            rule(r"(λ|Π|\|\}|\{\||\\/|/\\|=>|~|\}|\|]|\||\{<|\{|`|_|]|\[\||\[>|\[<|\[|\?\?|\?|>\}|>]|>|=|<->|<-|<|;;|;|:>|:=|::|:|\.\.|\.|->|-\.|-|,|\+|\*|\)|\(|&&|&|#|!=)").token(T::Operator),
            rule(r"([=<>@^|&+\*/$%-]|[!?~])?[!$%&*+\./:<=>?@^|~-]").token(T::Operator),
            rule(r"\b(unit|nat|bool|string|ascii|list)\b").token(T::KeywordType),
            rule(r"[^\W\d][\w']*").token(T::Name),
            rule(r"\d[\d_]*").token(T::LiteralNumberInteger),
            rule(r"0[xX][\da-fA-F][\da-fA-F_]*").token(T::LiteralNumberHex),
            rule(r"0[oO][0-7][0-7_]*").token(T::LiteralNumberOct),
            rule(r"0[bB][01][01_]*").token(T::LiteralNumberBin),
            rule(r"-?\d[\d_]*(.[\d_]*)?([eE][+\-]?\d[\d_]*)").token(T::LiteralNumberFloat),
            rule(r#"'(?:(\\[\\\"'ntbr ])|(\\[0-9]{3})|(\\x[0-9a-fA-F]{2}))'"#).token(T::LiteralStringChar),
            rule(r"'.'").token(T::LiteralStringChar),
            rule(r"'").token(T::Keyword),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["string"]),
            rule(r"[~?][a-z][\w\']*:").token(T::Name),
        ]),
        ("comment", &[
            rule(r"[^(*)]+").token(T::Comment),
            rule(r"\(\*").token(T::Comment).push(&[]),
            rule(r"\*\)").token(T::Comment).pop(1),
            rule(r"[(*)]").token(T::Comment),
        ]),
    ],
};
