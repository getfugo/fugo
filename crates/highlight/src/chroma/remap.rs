//! Token type remapping (Chroma's `remap.go`, `TypeRemappingLexer`): a token of type `from`
//! whose text is one of the listed words (or any text, with no words) becomes type `to`.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Config, Lexer, Registry, Token, TokeniseOptions};
use crate::token::TokenType;

/// `TypeRemappingLexer(lexer, mapping)`.
pub(crate) struct TypeRemappingLexer {
    lexer: Arc<dyn Lexer>,
    /// from → (word → to); the word `""` stands for any text.
    lut: HashMap<TokenType, HashMap<String, TokenType>>,
}

impl TypeRemappingLexer {
    /// `mapping`: `(from, to, words)`; later entries win, as in Chroma's lookup table.
    pub fn new(lexer: Arc<dyn Lexer>, mapping: &[(TokenType, TokenType, &[&str])]) -> Self {
        let mut lut: HashMap<TokenType, HashMap<String, TokenType>> = HashMap::new();
        for (from, to, words) in mapping {
            let km = lut.entry(*from).or_default();
            if words.is_empty() {
                km.insert(String::new(), *to);
            } else {
                for w in *words {
                    km.insert((*w).to_owned(), *to);
                }
            }
        }
        Self { lexer, lut }
    }
}

impl Lexer for TypeRemappingLexer {
    fn config(&self) -> &Config {
        self.lexer.config()
    }

    fn analyse_text(&self, text: &str) -> f32 {
        self.lexer.analyse_text(text)
    }

    fn tokenise(&self, reg: &Registry, opts: Option<&TokeniseOptions>, text: &str) -> Vec<Token> {
        let mut tokens = self.lexer.tokenise(reg, opts, text);
        for t in &mut tokens {
            if let Some(k) = self.lut.get(&t.ty)
                && let Some(to) = k.get(&t.value).or_else(|| k.get(""))
            {
                t.ty = *to;
            }
        }
        tokens
    }
}
