//! A language embedded in another (Chroma's `delegate.go`, `DelegatingLexer`): the language
//! lexer runs first and marks what it does not handle as `Other`; the `Other` text is joined
//! and tokenised by the root lexer, and the two token streams are merged.

use std::sync::Arc;

use super::{Config, Lexer, Registry, Token, TokeniseOptions, coalesce};
use crate::token::TokenType;

/// `DelegatingLexer(root, language)`.
pub(crate) struct DelegatingLexer {
    pub root: Arc<dyn Lexer>,
    pub language: Arc<dyn Lexer>,
}

/// Where language tokens go in the root lexer's output (byte offsets in the text).
struct Insertion {
    start: usize,
    end: usize,
    tokens: Vec<Token>,
}

impl Lexer for DelegatingLexer {
    fn config(&self) -> &Config {
        self.language.config()
    }

    fn analyse_text(&self, text: &str) -> f32 {
        self.root.analyse_text(text)
    }

    fn tokenise(&self, reg: &Registry, opts: Option<&TokeniseOptions>, text: &str) -> Vec<Token> {
        let tokens = coalesce(self.language.tokenise(reg, opts, text));
        // The insertions, and the `Other` text.
        let mut others = String::new();
        let mut insertions: Vec<Insertion> = Vec::new();
        let mut offset = 0usize;
        let mut last: Option<TokenType> = None;
        for t in tokens {
            if t.ty == TokenType::Other {
                if last.is_some_and(|l| l != TokenType::Other)
                    && let Some(i) = insertions.last_mut()
                {
                    i.end = offset;
                }
                others.push_str(&t.value);
            } else {
                if last.is_none_or(|l| l == TokenType::Other) {
                    insertions.push(Insertion {
                        start: offset,
                        end: 0,
                        tokens: Vec::new(),
                    });
                }
                if let Some(i) = insertions.last_mut() {
                    i.tokens.push(t.clone());
                }
            }
            last = Some(t.ty);
            offset += t.value.len();
        }
        if insertions.is_empty() {
            return self.root.tokenise(reg, opts, text);
        }

        let root_tokens = coalesce(self.root.tokenise(reg, opts, &others));

        // Interleave the two (Chroma keeps the offset signed: the last insertion's `end` is
        // left at 0 when the text ends inside it).
        let mut out = Vec::new();
        let mut offset: i64 = 0;
        let mut root_tokens = root_tokens.into_iter();
        let mut insertions = insertions.into_iter();
        let mut t = root_tokens.next();
        let mut i = insertions.next();
        while t.is_some() || i.is_some() {
            let take_insertion = match (&t, &i) {
                (None, _) => true,
                (Some(tok), Some(ins)) => to_i64(ins.start) < offset + to_i64(tok.value.len()),
                (Some(_), None) => false,
            };
            if take_insertion {
                let ins = i.take().expect("insertion");
                let (l, r) = split_token(t.take(), to_i64(ins.start) - offset);
                if let Some(l) = l {
                    offset += to_i64(l.value.len());
                    out.push(l);
                }
                t = r;
                offset += to_i64(ins.end) - to_i64(ins.start);
                out.extend(ins.tokens);
                if t.is_none() {
                    t = root_tokens.next();
                }
                i = insertions.next();
            } else {
                let tok = t.take().expect("token");
                offset += to_i64(tok.value.len());
                out.push(tok);
                t = root_tokens.next();
            }
        }
        out
    }
}

fn to_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// `splitToken`: the token split at a byte offset.
fn split_token(t: Option<Token>, offset: i64) -> (Option<Token>, Option<Token>) {
    let Some(t) = t else {
        return (None, None);
    };
    if offset == 0 {
        return (None, Some(t));
    }
    match usize::try_from(offset) {
        Ok(o) if o == t.value.len() => (Some(t), None),
        Ok(o) if o < t.value.len() && t.value.is_char_boundary(o) => {
            let (a, b) = t.value.split_at(o);
            (Some(Token::new(t.ty, a)), Some(Token::new(t.ty, b)))
        }
        _ => (None, Some(t)),
    }
}
