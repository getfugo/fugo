//! Chroma's lexing engine and lexers (github.com/alecthomas/chroma v2.19.0, MIT).
//!
//! A lexer is a regex state machine (Pygments' `RegexLexer`): in its current state it tries
//! the state's rules in order at the current position, the first rule whose pattern matches
//! there emits tokens (`token`, `bygroups`, `using`, `usingself`, `usingbygroup`) and changes
//! the state stack (`push`, `pop`; `include` and `combined` are resolved when the lexer is
//! compiled). Input no rule matches becomes an `Error` token, one character at a time; an
//! unmatched newline outside the start state resets the stack. Patterns are .NET regular
//! expressions ([`crate::regexp2`]), anchored at the position (`\G`).
//!
//! The lexers are Chroma's: the 255 XML lexers it embeds (`embedded/`), converted to Rust in
//! `lexers/` ([`defs`]), and its lexers written in Go, ported in [`golexers`] (template
//! languages delegating to HTML, Markdown, Go, …). [`Registry`] finds a lexer the way Chroma's
//! `lexers.Get` does (name, alias, then file name patterns by priority) and guesses one from
//! content (`Analyse`).
//!
//! The engine is Chroma's `lexer.go`, `regexp.go`, `mutators.go`, `emitters.go`,
//! `coalesce.go`, `delegate.go`, `remap.go`, `registry.go` and `serialise.go`, rewritten.

mod defs;
mod delegate;
mod glob;
pub(crate) mod golexers;
mod lexers;
mod regex_lexer;
mod registry;
mod remap;
mod stack;

pub(crate) use delegate::DelegatingLexer;
pub(crate) use regex_lexer::{Emitter, LexerState, RegexLexer, Rule, Rules};
pub(crate) use registry::Registry;
pub(crate) use remap::TypeRemappingLexer;

use crate::token::TokenType;

/// A token: its type and text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub ty: TokenType,
    pub value: String,
}

impl Token {
    pub(crate) fn new(ty: TokenType, value: impl Into<String>) -> Self {
        Self {
            ty,
            value: value.into(),
        }
    }

    /// Chroma's `EOF` (the zero `Token`): where an iterator ends. Only HTTP's lexer emits it
    /// before the end of its tokens (`httpBodyContentTyper`, when the body starts).
    pub(crate) fn is_eof(&self) -> bool {
        self.ty == TokenType::EOFType && self.value.is_empty()
    }
}

/// The tokens of a nested lexer's iterator, up to its first `EOF`: the consumers of a nested
/// iterator (the iterator stack of `LexerState.Iterator`, `Concaterator`) drop it there, so
/// an HTTP message inside another language loses its body and the tokens after it.
pub(crate) fn until_eof(tokens: Vec<Token>) -> impl Iterator<Item = Token> {
    tokens.into_iter().take_while(|t| !t.is_eof())
}

/// A lexer's configuration (Chroma's `Config`).
#[derive(Clone, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Chroma's `Config` fields, as in its lexer files"
)]
pub(crate) struct Config {
    pub name: String,
    pub aliases: Vec<String>,
    pub filenames: Vec<String>,
    pub alias_filenames: Vec<String>,
    pub mime_types: Vec<String>,
    pub case_insensitive: bool,
    pub dot_all: bool,
    pub not_multiline: bool,
    pub ensure_nl: bool,
    /// 0 counts as 1.
    pub priority: f32,
    pub analyse: Option<AnalyseConfig>,
}

impl Config {
    /// The priority used to order lexers matching the same file name.
    pub fn effective_priority(&self) -> f32 {
        if self.priority == 0.0 {
            1.0
        } else {
            self.priority
        }
    }
}

/// Content analysis by regular expressions (`[config.analyse]`).
#[derive(Clone, Debug, Default)]
pub(crate) struct AnalyseConfig {
    pub regexes: Vec<(String, f32)>,
    /// The first matching regex decides, instead of the sum.
    pub first: bool,
}

/// How a text is tokenised (Chroma's `TokeniseOptions`; `None` is its defaults: state `root`,
/// line ends normalised, not nested).
#[derive(Clone, Debug)]
pub(crate) struct TokeniseOptions {
    pub state: String,
    /// Tokenising part of another lexer's input: no newline is added (`EnsureNL`).
    pub nested: bool,
    pub ensure_lf: bool,
}

impl TokeniseOptions {
    /// What `using` and `usingself` pass: the given state, nested.
    pub fn nested(state: &str) -> Self {
        Self {
            state: state.to_owned(),
            nested: true,
            ensure_lf: false,
        }
    }
}

/// A lexer (Chroma's `Lexer` interface).
pub(crate) trait Lexer: Send + Sync {
    fn config(&self) -> &Config;

    /// The tokens of `text` (Chroma's `Tokenise`, run to the end).
    fn tokenise(&self, reg: &Registry, opts: Option<&TokeniseOptions>, text: &str) -> Vec<Token>;

    /// How likely `text` is in this lexer's language, 0 to 1 (`AnalyseText`).
    fn analyse_text(&self, _text: &str) -> f32 {
        0.0
    }
}

/// Chroma's `Coalesce`: drops empty tokens and joins runs of one type (while the run is
/// shorter than 8 KiB). At an `EOF` before the end (HTTP's body; [`Token::is_eof`]) it hands
/// out the pending run, and the next token starts a new one; with no run pending, `EOF` ends
/// the stream (the Go implementation's formatter reads up to the first `EOF`).
pub(crate) fn coalesce(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    // Go's `prev != EOF`: the last token of `out` is a run that may grow.
    let mut pending = false;
    for t in tokens {
        if t.is_eof() {
            if !pending {
                break;
            }
            pending = false;
            continue;
        }
        if t.value.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(prev) if pending && prev.ty == t.ty && prev.value.len() < 8192 => {
                prev.value.push_str(&t.value);
            }
            _ => out.push(t),
        }
        pending = true;
    }
    out
}

/// Chroma's `ensureLF`: `\r\n` and `\r` become `\n`.
pub(crate) fn ensure_lf(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                continue;
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests;
