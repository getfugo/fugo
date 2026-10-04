//! Per-page CSS purging (`purge_css`): a style sheet cut down, for each page, to the rules that
//! page can use, as PurgeCSS does for a whole site.
//!
//! A style sheet is parsed once into a [`PurgePlan`]: the selectors and declarations of every
//! style rule printed (compactly) on their own, with the class, id and element names each
//! selector needs. For a page, [`PurgePlan::purge`] keeps the selectors whose names the page
//! uses ([`PageNames`]) and joins the printed pieces, so no page parses CSS.
//!
//! - A selector is kept when the page uses every class, id and element name it requires. Names
//!   inside `:not()` do not count; of `:is()`, `:where()`, `:has()` and `:-webkit-any()` one
//!   argument is enough. Attribute selectors, pseudo-classes and pseudo-elements count as used.
//! - A style rule keeps its kept selectors and goes without any; a group rule (`@media`,
//!   `@supports`, `@container`, `@layer`, `@scope`, …) goes when nothing inside it is kept.
//!   Other rules (`@font-face`, `@keyframes`, `@import`, `@property`, …) are always kept, and so
//!   is a style rule with nested rules, whole, when one of its selectors is.
//! - A style sheet lightningcss rejects is compiled rule by rule (its top-level rules, as the
//!   tolerant minifier cuts it): a rule lightningcss rejects is kept as written on every page
//!   (an invalid selector such as `.a::before.b` makes browsers ignore the rule anyway).
//! - Besides the page's own names, a name is used when the safelist names it or when it is a
//!   word of the purge's content (the scripts that add classes at run time). Greedy patterns
//!   keep every selector whose text they match; blocklisted names drop their selectors.
//! - With `variables`, a custom property declaration is dropped unless a kept declaration
//!   references the property (directly or through other properties), an always-kept rule does,
//!   or the page or the content mentions it (`style="color: var(--x)"`, scripts).

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, PoisonError};

use lightningcss::declaration::DeclarationBlock;
use lightningcss::properties::Property;
use lightningcss::properties::custom::CustomPropertyName;
use lightningcss::rules::{CssRule, CssRuleList};
use lightningcss::selector::{Component, Selector};
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::targets::Targets;
use lightningcss::traits::ToCss;
use regex::Regex;

use crate::MinifyError;

mod compiler;
mod plan;

/// The prefix of the placeholders `purge_css` returns (`__nh_purge_<n>__`); the publisher
/// replaces them with the purged CSS of each page.
pub const PURGE_PREFIX: &str = "__nh_purge_";

/// What kind of name a selector requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameKind {
    /// An element name (`div`), lower-cased.
    Tag,
    Class,
    Id,
}

/// The names a page uses.
#[derive(Clone, Debug, Default)]
pub struct PageNames {
    pub tags: BTreeSet<String>,
    pub classes: BTreeSet<String>,
    pub ids: BTreeSet<String>,
    /// Words that count as names of any kind (the page's scripts), and the custom properties
    /// the page mentions (`--name`).
    pub words: BTreeSet<String>,
}

impl PageNames {
    fn has(&self, kind: NameKind, name: &str) -> bool {
        let names = match kind {
            NameKind::Tag => &self.tags,
            NameKind::Class => &self.classes,
            NameKind::Id => &self.ids,
        };
        names.contains(name) || self.words.contains(name)
    }
}

/// The words of `text` (scripts, templates in scripts): runs of letters, digits, `_`, `-`, `:`
/// and `/`, and the parts of a run split at `:` and `/` (`md:flex` is `md:flex`, `md` and
/// `flex`).
pub fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | ':' | '/')))
        .filter(|w| !w.is_empty())
        .flat_map(|w| {
            let parts = w
                .contains([':', '/'])
                .then(|| w.split([':', '/']).filter(|p| !p.is_empty()));
            std::iter::once(w).chain(parts.into_iter().flatten())
        })
}

/// What a purge keeps besides the names a page uses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PurgeOptions {
    /// Names (classes, ids, elements) always used: exact, or a regular expression between
    /// slashes (`/^bs-/`).
    pub safelist: Vec<String>,
    /// Selectors always kept: those whose text contains the string, or matches the `/regular
    /// expression/`.
    pub greedy: Vec<String>,
    /// Names whose selectors are dropped even when used: exact, or a `/regular expression/`.
    pub blocklist: Vec<String>,
    /// Drop custom property declarations nothing references.
    pub variables: bool,
    /// Print every declaration without `!important`.
    pub drop_important: bool,
}

/// A name, a substring or a regular expression.
enum Pattern {
    Text(String),
    Regex(Regex),
}

impl Pattern {
    fn parse(s: &str) -> Result<Self, MinifyError> {
        match s.strip_prefix('/').and_then(|r| r.strip_suffix('/')) {
            Some(re) => Regex::new(re)
                .map(Self::Regex)
                .map_err(|e| MinifyError::Purge(format!("{s}: {e}"))),
            None => Ok(Self::Text(s.to_owned())),
        }
    }

    fn is_name(&self, name: &str) -> bool {
        match self {
            Self::Text(t) => t == name,
            Self::Regex(r) => r.is_match(name),
        }
    }

    fn in_text(&self, text: &str) -> bool {
        match self {
            Self::Text(t) => text.contains(t.as_str()),
            Self::Regex(r) => r.is_match(text),
        }
    }
}

fn patterns(list: &[String]) -> Result<Vec<Pattern>, MinifyError> {
    list.iter().map(|s| Pattern::parse(s)).collect()
}

/// A style sheet prepared for purging per page.
#[derive(Debug)]
pub struct PurgePlan {
    names: Vec<Name>,
    vars: Vec<String>,
    selectors: Vec<Sel>,
    items: Vec<Item>,
    /// Custom properties referenced by always-kept rules or named by the content.
    always_vars: Vec<u32>,
    variables: bool,
}

#[derive(Debug)]
struct Name {
    kind: NameKind,
    text: String,
    /// Safelisted, or a word of the content.
    always: bool,
}

#[derive(Debug)]
struct Sel {
    text: String,
    need: Need,
    keep: Keep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keep {
    Check,
    /// Matched by a greedy pattern.
    Always,
    /// Requires a blocklisted name.
    Never,
}

/// The names a selector requires.
#[derive(Debug)]
enum Need {
    Name(u32),
    All(Vec<Need>),
    Any(Vec<Need>),
}

impl Need {
    fn met(&self, used: &[bool]) -> bool {
        match self {
            Self::Name(n) => used[*n as usize],
            Self::All(all) => all.iter().all(|n| n.met(used)),
            Self::Any(any) => any.iter().any(|n| n.met(used)),
        }
    }

    fn any_name(&self, f: &impl Fn(u32) -> bool) -> bool {
        match self {
            Self::Name(n) => f(*n),
            Self::All(v) | Self::Any(v) => v.iter().any(|n| n.any_name(f)),
        }
    }
}

#[derive(Debug)]
struct Decl {
    text: String,
    /// The custom property it declares.
    defines: Option<u32>,
    /// The custom properties its value references.
    refs: Vec<u32>,
}

#[derive(Debug)]
enum Item {
    /// Always kept.
    Raw(String),
    Style {
        sels: Vec<u32>,
        decls: Vec<Decl>,
    },
    /// Kept whole when a selector is (style rules with nested rules).
    Opaque {
        sels: Vec<u32>,
        text: String,
        refs: Vec<u32>,
    },
    /// `None`: the rules are printed without a wrapper (`@media all` when minified).
    Group {
        prelude: Option<String>,
        items: Vec<Item>,
    },
}

fn printer(targets: Targets) -> PrinterOptions<'static> {
    PrinterOptions {
        minify: true,
        targets,
        ..PrinterOptions::default()
    }
}

/// The custom properties referenced (`var(--name`) in printed CSS.
fn var_refs(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices("var(").filter_map(|(at, _)| {
        let rest = text[at + 4..].trim_start();
        let rest = rest.strip_prefix("--")?;
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_') || !c.is_ascii()))
            .unwrap_or(rest.len());
        let start = text.len() - rest.len() - 2;
        Some(&text[start..start + 2 + end])
    })
}

struct Compiler<'c> {
    safelist: Vec<Pattern>,
    greedy: Vec<Pattern>,
    blocklist: Vec<Pattern>,
    content: BTreeSet<&'c str>,
    drop_important: bool,
    names: Vec<Name>,
    blocked: Vec<bool>,
    name_ids: HashMap<(NameKind, String), u32>,
    vars: Vec<String>,
    var_ids: HashMap<String, u32>,
    selectors: Vec<Sel>,
    always_vars: Vec<u32>,
    targets: Targets,
}

/// The plans of a build's `purge_css` calls: the templates register them, the publisher
/// replaces their placeholders with the CSS each page uses.
#[derive(Debug, Default)]
pub struct CssPurges {
    inner: Mutex<Registry>,
}

#[derive(Debug, Default)]
struct Registry {
    by_key: HashMap<u64, usize>,
    plans: Vec<Arc<PurgePlan>>,
}

impl CssPurges {
    /// The placeholder of the plan registered under `key`, compiled by `compile` on first use.
    ///
    /// # Errors
    /// `compile`'s.
    pub fn placeholder(
        &self,
        key: u64,
        compile: impl FnOnce() -> Result<PurgePlan, MinifyError>,
    ) -> Result<String, MinifyError> {
        let mut r = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let id = match r.by_key.get(&key) {
            Some(&id) => id,
            None => {
                r.plans.push(Arc::new(compile()?));
                let id = r.plans.len() - 1;
                r.by_key.insert(key, id);
                id
            }
        };
        Ok(format!("{PURGE_PREFIX}{id}__"))
    }

    /// `text` with every placeholder replaced by the CSS its plan keeps for the page `names`
    /// describes (computed once, when there is a placeholder); `None` without placeholders.
    ///
    /// # Errors
    /// A placeholder of no plan.
    pub fn resolve(
        &self,
        text: &str,
        names: impl FnOnce() -> PageNames,
    ) -> Result<Option<String>, MinifyError> {
        if !text.contains(PURGE_PREFIX) {
            return Ok(None);
        }
        let page = names();
        let plans = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .plans
            .clone();
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find(PURGE_PREFIX) {
            out.push_str(&rest[..at]);
            let after = &rest[at + PURGE_PREFIX.len()..];
            let digits = after
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after.len());
            let plan = after
                .get(digits..)
                .filter(|t| t.starts_with("__"))
                .and_then(|_| after[..digits].parse::<usize>().ok())
                .and_then(|id| plans.get(id));
            let Some(plan) = plan else {
                let end = (at + PURGE_PREFIX.len() + digits + 2).min(rest.len());
                return Err(MinifyError::Purge(format!(
                    "unknown placeholder {}",
                    &rest[at..end]
                )));
            };
            out.push_str(&plan.purge(&page));
            rest = &after[digits + 2..];
        }
        out.push_str(rest);
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests;
