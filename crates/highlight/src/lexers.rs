//! Language lookup: which Chroma lexer a fence's language names (Chroma's `lexers.Get`, as
//! in Go), content analysis for `guessSyntax`, and tokenising with Chroma's coalescing.

use std::sync::Arc;

use crate::chroma::{self, Config, RegexLexer, Registry, Rule, Rules, Token};
use crate::token::TokenType;

/// A Chroma lexer, by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Lexer<'a> {
    /// Chroma's lexer name (`Go HTML Template`, `plaintext`).
    pub name: &'a str,
}

/// Chroma's lexers.
pub(crate) struct Languages {
    registry: Registry,
    /// Chroma's `lexers.Fallback`: plain text, for `guessSyntax` when no analyser scores.
    fallback: Arc<dyn chroma::Lexer>,
}

impl std::fmt::Debug for Languages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Languages")
            .field("registry", &self.registry)
            .finish_non_exhaustive()
    }
}

/// A lexer of the registry, ready to tokenise.
pub(crate) struct LexerRef<'a> {
    lexer: &'a Arc<dyn chroma::Lexer>,
    registry: &'a Registry,
}

impl<'a> LexerRef<'a> {
    pub fn name(&self) -> &'a str {
        &self.lexer.config().name
    }

    /// Chroma's coalesced tokens of `code` (Go's `chroma.Coalesce(lexer).Tokenise(nil, code)`).
    pub fn tokens(&self, code: &str) -> Vec<Token> {
        chroma::coalesce(self.lexer.tokenise(self.registry, None, code))
    }
}

/// `lexers.PlaintextRules`.
fn plaintext_rules() -> Rules {
    let rule = |p: &str| Rule {
        pattern: p.to_owned(),
        emitter: Some(chroma::Emitter::Token(TokenType::Text)),
        mutator: None,
    };
    Rules::from([("root".to_owned(), vec![rule(".+"), rule("\\n")])])
}

impl Languages {
    /// Registers every Chroma lexer (their rules compile on first use).
    pub fn load() -> Self {
        let fallback = RegexLexer::from_code(
            Config {
                name: "fallback".to_owned(),
                filenames: vec!["*".to_owned()],
                priority: -1.0,
                ..Config::default()
            },
            plaintext_rules,
        );
        Self {
            registry: chroma::golexers::registry(),
            fallback: Arc::new(fallback),
        }
    }

    /// The lexer `lang` names (Chroma's `lexers.Get`).
    pub fn get(&self, lang: &str) -> Option<LexerRef<'_>> {
        self.registry.get(lang).map(|lexer| LexerRef {
            lexer,
            registry: &self.registry,
        })
    }

    /// The Chroma lexer `lang` names.
    pub fn lexer(&self, lang: &str) -> Option<Lexer<'_>> {
        self.get(lang).map(|l| Lexer { name: l.name() })
    }

    /// The lexer whose analyser scores `code` highest, else the fallback (Go's
    /// `lexers.Analyse` + `lexers.Fallback`).
    pub fn analyse(&self, code: &str) -> LexerRef<'_> {
        LexerRef {
            lexer: self.registry.analyse(code).unwrap_or(&self.fallback),
            registry: &self.registry,
        }
    }

    /// Every lexer name, in registration order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.registry
            .lexers()
            .iter()
            .map(|l| l.config().name.as_str())
    }
}
