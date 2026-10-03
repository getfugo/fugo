//! The lexer registry (Chroma's `registry.go` and `lexers/lexers.go`): every lexer in Chroma's
//! registration order, found by name or alias (exactly, then lower-cased), then by file name
//! (`filename.<name>`, then `<name>`, by file name pattern and priority), by MIME type, or by
//! content analysis.

use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

use super::{Lexer, glob};

/// Suffixes a file name may carry after a lexer's pattern (`ignoredSuffixes`).
const IGNORED_SUFFIXES: &[&str] = &[
    "~",
    ".bak",
    ".old",
    ".orig",
    ".dpkg-dist",
    ".dpkg-old",
    ".ucf-dist",
    ".ucf-new",
    ".ucf-old",
    ".rpmnew",
    ".rpmorig",
    ".rpmsave",
    ".in",
];

/// The registered lexers (`LexerRegistry`).
#[derive(Default)]
pub(crate) struct Registry {
    lexers: Vec<Arc<dyn Lexer>>,
    by_name: HashMap<String, Arc<dyn Lexer>>,
    by_alias: HashMap<String, Arc<dyn Lexer>>,
    /// [`Registry::get`]'s file name lookups, by name: the lexer's index in `lexers`, `None`
    /// for no lexer. Go caches `lexers.Get` per name (`markup/highlight/chromalexers`): a
    /// name that is no lexer's tries every file name pattern, and fences name such languages
    /// often (`output`, `console`, …). Shared by the threads that render.
    by_filename: RwLock<HashMap<String, Option<usize>>>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("lexers", &self.lexers.len())
            .finish_non_exhaustive()
    }
}

impl Registry {
    /// Registers a lexer, replacing (in place) one of the same name (`Register`).
    pub fn register(&mut self, lexer: Arc<dyn Lexer>) {
        let config = lexer.config();
        self.by_name.insert(config.name.clone(), Arc::clone(&lexer));
        self.by_name
            .insert(config.name.to_lowercase(), Arc::clone(&lexer));
        for alias in &config.aliases {
            self.by_alias.insert(alias.clone(), Arc::clone(&lexer));
            self.by_alias
                .insert(alias.to_lowercase(), Arc::clone(&lexer));
        }
        match self
            .lexers
            .iter()
            .position(|l| l.config().name == config.name)
        {
            Some(i) => self.lexers[i] = lexer,
            None => self.lexers.push(lexer),
        }
        self.by_filename
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Every lexer, in registration order.
    pub fn lexers(&self) -> &[Arc<dyn Lexer>] {
        &self.lexers
    }

    /// The lexer for a name, alias or file extension (`Get`; the file name part cached per
    /// name, as Go's `chromalexers.Get` caches the whole lookup).
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Lexer>> {
        if let Some(l) = self.by_name.get(name).or_else(|| self.by_alias.get(name)) {
            return Some(l);
        }
        let lower = name.to_lowercase();
        if let Some(l) = self
            .by_name
            .get(&lower)
            .or_else(|| self.by_alias.get(&lower))
        {
            return Some(l);
        }
        let cached = self
            .by_filename
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .copied();
        if let Some(index) = cached {
            return index.map(|i| &self.lexers[i]);
        }
        let mut candidates = Vec::new();
        if let Some(l) = self.match_filename(&format!("filename.{name}")) {
            candidates.push(l);
        }
        if let Some(l) = self.match_filename(name) {
            candidates.push(l);
        }
        let found = best(candidates);
        let index = found.and_then(|l| self.lexers.iter().position(|x| Arc::ptr_eq(x, l)));
        self.by_filename
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(name.to_owned(), index);
        found
    }

    /// The best lexer for a file name (`Match`): primary patterns first, then alias patterns.
    pub fn match_filename(&self, filename: &str) -> Option<&Arc<dyn Lexer>> {
        // `filepath.Base`.
        let trimmed = filename.trim_end_matches('/');
        let base = match trimmed.rsplit_once('/') {
            Some((_, b)) => b,
            None if trimmed.is_empty() && !filename.is_empty() => "/",
            None if trimmed.is_empty() => ".",
            None => trimmed,
        };
        for alias in [false, true] {
            let mut matched = Vec::new();
            for lexer in &self.lexers {
                let config = lexer.config();
                let globs = if alias {
                    &config.alias_filenames
                } else {
                    &config.filenames
                };
                for g in globs {
                    // `{g}{suf}` matches only names ending with `suf`: the suffixes are literal
                    // (a `[` that `g` leaves open makes the pattern malformed, no match).
                    let hit = glob::matches(g, base) == Some(true)
                        || IGNORED_SUFFIXES.iter().any(|suf| {
                            base.ends_with(suf)
                                && glob::matches(&format!("{g}{suf}"), base) == Some(true)
                        });
                    if hit {
                        matched.push(lexer);
                    }
                }
            }
            if let Some(l) = best(matched) {
                return Some(l);
            }
        }
        None
    }

    /// The best lexer for a MIME type (`MatchMimeType`).
    pub fn match_mime_type(&self, mime: &str) -> Option<&Arc<dyn Lexer>> {
        let matched = self
            .lexers
            .iter()
            .flat_map(|l| {
                l.config()
                    .mime_types
                    .iter()
                    .filter(|m| *m == mime)
                    .map(move |_| l)
            })
            .collect();
        best(matched)
    }

    /// The lexer whose analyser scores `text` highest (`Analyse`; ties go to the first).
    pub fn analyse(&self, text: &str) -> Option<&Arc<dyn Lexer>> {
        let mut picked = None;
        let mut highest = 0.0f32;
        for lexer in &self.lexers {
            let weight = lexer.analyse_text(text);
            if weight > highest {
                picked = Some(lexer);
                highest = weight;
            }
        }
        picked
    }
}

/// The highest-priority lexer (`sort.Sort(PrioritisedLexers)`; stable for the small lists
/// Chroma sorts, as Go's insertion sort is).
fn best(mut lexers: Vec<&Arc<dyn Lexer>>) -> Option<&Arc<dyn Lexer>> {
    lexers.sort_by(|a, b| {
        b.config()
            .effective_priority()
            .partial_cmp(&a.config().effective_priority())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    lexers.into_iter().next()
}
