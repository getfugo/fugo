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

impl PurgePlan {
    /// Prepares `css` for purging with `options`; `content` are texts (scripts) whose words
    /// count as used names on every page. The pieces are printed for `targets` (the
    /// minifier's, [`crate::Minifier::css_targets`]: without browsers lightningcss prints the
    /// newest syntax, such as media query ranges).
    ///
    /// # Errors
    /// An invalid regular expression, or CSS that cannot be printed.
    pub fn compile(
        css: &str,
        options: &PurgeOptions,
        content: &[&str],
        targets: Targets,
    ) -> Result<Self, MinifyError> {
        // A byte order mark (Sass writes one before non-ASCII CSS) would be read as part of the
        // first selector, and it means nothing inside a page's `<style>`.
        let css = css.strip_prefix('\u{feff}').unwrap_or(css);
        let mut c = Compiler {
            safelist: patterns(&options.safelist)?,
            greedy: patterns(&options.greedy)?,
            blocklist: patterns(&options.blocklist)?,
            content: content.iter().flat_map(|t| words(t)).collect(),
            drop_important: options.drop_important,
            names: Vec::new(),
            blocked: Vec::new(),
            name_ids: HashMap::new(),
            vars: Vec::new(),
            var_ids: HashMap::new(),
            selectors: Vec::new(),
            always_vars: Vec::new(),
            targets,
        };
        let items = match StyleSheet::parse(css, ParserOptions::default()) {
            Ok(sheet) => c.rules(&sheet.rules)?,
            Err(_) => {
                let mut items = Vec::new();
                for (from, to) in crate::css::split(css) {
                    let chunk = &css[from..to];
                    match StyleSheet::parse(chunk, ParserOptions::default()) {
                        Ok(sheet) => items.extend(c.rules(&sheet.rules)?),
                        Err(_) => {
                            let text = chunk.trim().to_owned();
                            let refs = c.refs(&text);
                            c.always_vars.extend(refs);
                            items.push(Item::Raw(text));
                        }
                    }
                }
                items
            }
        };
        let mut always_vars = c.always_vars;
        for (i, v) in c.vars.iter().enumerate() {
            if c.content.contains(v.as_str()) {
                always_vars.push(u32::try_from(i).unwrap_or(u32::MAX));
            }
        }
        Ok(Self {
            names: c.names,
            vars: c.vars,
            selectors: c.selectors,
            items,
            always_vars,
            variables: options.variables,
        })
    }

    /// The CSS `page` can use, printed compactly.
    #[must_use]
    pub fn purge(&self, page: &PageNames) -> String {
        let used: Vec<bool> = self
            .names
            .iter()
            .map(|n| n.always || page.has(n.kind, &n.text))
            .collect();
        let kept: Vec<bool> = self
            .selectors
            .iter()
            .map(|s| match s.keep {
                Keep::Always => true,
                Keep::Never => false,
                Keep::Check => s.need.met(&used),
            })
            .collect();
        let vars = self.variables.then(|| self.used_vars(&kept, page));
        let mut out = String::new();
        self.write(&self.items, &kept, vars.as_deref(), &mut out);
        out
    }

    /// The custom properties a page needs, given the kept selectors.
    fn used_vars(&self, kept: &[bool], page: &PageNames) -> Vec<bool> {
        let mut used = vec![false; self.vars.len()];
        let mut defs: Vec<Vec<u32>> = vec![Vec::new(); self.vars.len()];
        let mut stack = self.always_vars.clone();
        for (i, v) in self.vars.iter().enumerate() {
            if page.words.contains(v) {
                stack.push(u32::try_from(i).unwrap_or(u32::MAX));
            }
        }
        self.var_roots(&self.items, kept, &mut defs, &mut stack);
        while let Some(v) = stack.pop() {
            let Some(slot) = used.get_mut(v as usize) else {
                continue;
            };
            if !*slot {
                *slot = true;
                stack.extend(&defs[v as usize]);
            }
        }
        used
    }

    fn var_roots(
        &self,
        items: &[Item],
        kept: &[bool],
        defs: &mut [Vec<u32>],
        roots: &mut Vec<u32>,
    ) {
        for item in items {
            match item {
                Item::Raw(_) => {}
                Item::Style { sels, decls } => {
                    if sels.iter().any(|&s| kept[s as usize]) {
                        for d in decls {
                            match d.defines {
                                Some(v) => defs[v as usize].extend(&d.refs),
                                None => roots.extend(&d.refs),
                            }
                        }
                    }
                }
                Item::Opaque { sels, refs, .. } => {
                    if sels.iter().any(|&s| kept[s as usize]) {
                        roots.extend(refs);
                    }
                }
                Item::Group { items, .. } => self.var_roots(items, kept, defs, roots),
            }
        }
    }

    fn write(&self, items: &[Item], kept: &[bool], vars: Option<&[bool]>, out: &mut String) {
        for item in items {
            match item {
                Item::Raw(text) => out.push_str(text),
                Item::Opaque { sels, text, .. } => {
                    if sels.iter().any(|&s| kept[s as usize]) {
                        out.push_str(text);
                    }
                }
                Item::Style { sels, decls } => {
                    let start = out.len();
                    for &s in sels.iter().filter(|&&s| kept[s as usize]) {
                        if out.len() > start {
                            out.push(',');
                        }
                        out.push_str(&self.selectors[s as usize].text);
                    }
                    if out.len() == start {
                        continue;
                    }
                    out.push('{');
                    let body = out.len();
                    for d in decls {
                        if let (Some(v), Some(used)) = (d.defines, vars)
                            && !used[v as usize]
                        {
                            continue;
                        }
                        if out.len() > body {
                            out.push(';');
                        }
                        out.push_str(&d.text);
                    }
                    if out.len() == body {
                        out.truncate(start);
                    } else {
                        out.push('}');
                    }
                }
                Item::Group { prelude, items } => {
                    let start = out.len();
                    if let Some(p) = prelude {
                        out.push_str(p);
                        out.push('{');
                    }
                    let inner = out.len();
                    self.write(items, kept, vars, out);
                    if out.len() == inner {
                        out.truncate(start);
                    } else if prelude.is_some() {
                        out.push('}');
                    }
                }
            }
        }
    }
}

impl Compiler<'_> {
    fn print<T: ToCss>(&self, v: &T) -> Result<String, MinifyError> {
        v.to_css_string(printer(self.targets))
            .map_err(|e| MinifyError::Purge(e.to_string()))
    }

    fn rules(&mut self, rules: &CssRuleList<'_>) -> Result<Vec<Item>, MinifyError> {
        rules.0.iter().map(|r| self.rule(r)).collect()
    }

    fn rule(&mut self, rule: &CssRule<'_>) -> Result<Item, MinifyError> {
        // A group rule: its prelude (the rule printed without its rules) and its items.
        macro_rules! group {
            ($variant:ident, $r:expr) => {{
                let mut empty = $r.clone();
                empty.rules = CssRuleList(Vec::new());
                let head = self.print(&CssRule::$variant(empty))?;
                match head.strip_suffix("{}") {
                    Some(prelude) => Item::Group {
                        prelude: Some(prelude.to_owned()),
                        items: self.rules(&$r.rules)?,
                    },
                    None if head.is_empty() => Item::Group {
                        prelude: None,
                        items: self.rules(&$r.rules)?,
                    },
                    None => self.raw(rule)?,
                }
            }};
        }
        Ok(match rule {
            CssRule::Style(s) if s.rules.0.is_empty() && s.vendor_prefix.is_empty() => {
                let sels = s
                    .selectors
                    .0
                    .iter()
                    .map(|sel| self.selector(sel))
                    .collect::<Result<_, _>>()?;
                Item::Style {
                    sels,
                    decls: self.declarations(&s.declarations)?,
                }
            }
            CssRule::Style(s) => {
                let sels = s
                    .selectors
                    .0
                    .iter()
                    .map(|sel| self.selector(sel))
                    .collect::<Result<_, _>>()?;
                let text = self.print(rule)?;
                let refs = self.refs(&text);
                Item::Opaque { sels, text, refs }
            }
            CssRule::Media(r) => group!(Media, r),
            CssRule::Supports(r) => group!(Supports, r),
            CssRule::Container(r) => group!(Container, r),
            CssRule::LayerBlock(r) => group!(LayerBlock, r),
            CssRule::MozDocument(r) => group!(MozDocument, r),
            CssRule::Scope(r) => group!(Scope, r),
            CssRule::StartingStyle(r) => group!(StartingStyle, r),
            _ => self.raw(rule)?,
        })
    }

    fn raw(&mut self, rule: &CssRule<'_>) -> Result<Item, MinifyError> {
        let text = self.print(rule)?;
        let refs = self.refs(&text);
        self.always_vars.extend(refs);
        Ok(Item::Raw(text))
    }

    fn selector(&mut self, sel: &Selector<'_>) -> Result<u32, MinifyError> {
        let text = self.print(sel)?;
        let need = self.need(sel);
        let keep = if need.any_name(&|n| self.blocked[n as usize]) {
            Keep::Never
        } else if self.greedy.iter().any(|g| g.in_text(&text)) {
            Keep::Always
        } else {
            Keep::Check
        };
        self.selectors.push(Sel { text, need, keep });
        Ok(u32::try_from(self.selectors.len() - 1).unwrap_or(u32::MAX))
    }

    fn need(&mut self, sel: &Selector<'_>) -> Need {
        let mut all = Vec::new();
        for c in sel.iter_raw_match_order() {
            match c {
                Component::Class(n) => all.push(Need::Name(self.name(NameKind::Class, &n.0))),
                Component::ID(n) => all.push(Need::Name(self.name(NameKind::Id, &n.0))),
                Component::LocalName(n) => {
                    all.push(Need::Name(self.name(NameKind::Tag, &n.lower_name.0)));
                }
                Component::Is(list)
                | Component::Where(list)
                | Component::Has(list)
                | Component::Any(_, list) => {
                    all.push(Need::Any(list.iter().map(|s| self.need(s)).collect()));
                }
                _ => {}
            }
        }
        Need::All(all)
    }

    fn name(&mut self, kind: NameKind, text: &str) -> u32 {
        if let Some(&id) = self.name_ids.get(&(kind, text.to_owned())) {
            return id;
        }
        let always = self.safelist.iter().any(|p| p.is_name(text)) || self.content.contains(text);
        self.blocked
            .push(self.blocklist.iter().any(|p| p.is_name(text)));
        self.names.push(Name {
            kind,
            text: text.to_owned(),
            always,
        });
        let id = u32::try_from(self.names.len() - 1).unwrap_or(u32::MAX);
        self.name_ids.insert((kind, text.to_owned()), id);
        id
    }

    fn var(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.var_ids.get(name) {
            return id;
        }
        self.vars.push(name.to_owned());
        let id = u32::try_from(self.vars.len() - 1).unwrap_or(u32::MAX);
        self.var_ids.insert(name.to_owned(), id);
        id
    }

    fn refs(&mut self, text: &str) -> Vec<u32> {
        var_refs(text).map(|v| self.var(v)).collect()
    }

    fn declarations(&mut self, block: &DeclarationBlock<'_>) -> Result<Vec<Decl>, MinifyError> {
        let important = !self.drop_important;
        let all = block
            .declarations
            .iter()
            .map(|p| (p, false))
            .chain(block.important_declarations.iter().map(|p| (p, important)));
        let mut decls = Vec::new();
        for (p, important) in all {
            let text = p
                .to_css_string(important, printer(self.targets))
                .map_err(|e| MinifyError::Purge(e.to_string()))?;
            let defines = match p {
                Property::Custom(c) => match &c.name {
                    CustomPropertyName::Custom(d) => Some(self.var(&d.0)),
                    CustomPropertyName::Unknown(_) => None,
                },
                _ => None,
            };
            let refs = self.refs(&text);
            decls.push(Decl {
                text,
                defines,
                refs,
            });
        }
        Ok(decls)
    }
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
mod tests {
    use super::*;

    fn page(tags: &[&str], classes: &[&str], ids: &[&str], words: &[&str]) -> PageNames {
        let set = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect();
        PageNames {
            tags: set(tags),
            classes: set(classes),
            ids: set(ids),
            words: set(words),
        }
    }

    /// Printed for Safari 12 (no media query ranges).
    fn plan(css: &str, options: &PurgeOptions, content: &[&str]) -> PurgePlan {
        let targets = Targets {
            browsers: Some(lightningcss::targets::Browsers {
                safari: Some(12 << 16),
                ..Default::default()
            }),
            ..Targets::default()
        };
        PurgePlan::compile(css, options, content, targets).expect("compile")
    }

    #[test]
    fn selectors_need_their_names() {
        let p = plan(
            "body{margin:0}.a,.b{color:red}.a .c{color:blue}#x{top:0}p.a{left:0}\
             .n:not(.zz){x:y}:is(.a,.q) span{z:1}a:hover{color:green}[data-x]{w:1}",
            &PurgeOptions::default(),
            &[],
        );
        let out = plan_out(&p, &page(&["body", "p"], &["a", "n"], &[], &[]));
        assert_eq!(
            out,
            "body{margin:0}.a{color:red}p.a{left:0}.n:not(.zz){x:y}[data-x]{w:1}"
        );
        let out = plan_out(&p, &page(&["span", "a"], &["a", "b", "c"], &["x"], &[]));
        assert_eq!(
            out,
            ".a,.b{color:red}.a .c{color:#00f}#x{top:0}:is(.a,.q) span{z:1}a:hover{color:green}[data-x]{w:1}"
        );
    }

    fn plan_out(p: &PurgePlan, names: &PageNames) -> String {
        p.purge(names)
    }

    #[test]
    fn groups_and_other_rules() {
        let css = "@charset \"utf-8\";@font-face{font-family:F;src:url(f.woff2)}\
                   @media (min-width:768px){.a{color:red}.b{color:blue}}\
                   @supports (display:grid){.b{display:grid}}\
                   @keyframes k{0%{opacity:0}to{opacity:1}}";
        let p = plan(css, &PurgeOptions::default(), &[]);
        let out = p.purge(&page(&[], &["a"], &[], &[]));
        assert_eq!(
            out,
            "@font-face{font-family:F;src:url(f.woff2)}@media (min-width:768px){.a{color:red}}\
             @keyframes k{0%{opacity:0}to{opacity:1}}"
        );
    }

    #[test]
    fn safelist_content_greedy_blocklist() {
        let options = PurgeOptions {
            safelist: vec!["s".into(), "/^re-/".into()],
            greedy: vec!["/bs-dark/".into()],
            blocklist: vec!["/^no-/".into()],
            ..PurgeOptions::default()
        };
        let p = plan(
            ".s{a:1}.re-x{a:2}.shown{a:3}.bs-dark .zz{a:4}.no-x{a:5}.other{a:6}",
            &options,
            &["el.classList.add('shown')"],
        );
        assert_eq!(
            p.purge(&page(&[], &["no-x"], &[], &[])),
            ".s{a:1}.re-x{a:2}.shown{a:3}.bs-dark .zz{a:4}"
        );
        // Script words of the page count too.
        assert!(
            p.purge(&page(&[], &[], &[], &["other"]))
                .contains(".other{a:6}")
        );
    }

    #[test]
    fn unused_variables() {
        let css = ":root{--a:1px;--b:var(--a);--c:2px;--d:3px;--e:4px}.x{margin:var(--b)}\
                   .y{padding:var(--c)}@font-face{font-family:F;font-weight:var(--e)}";
        let options = PurgeOptions {
            variables: true,
            ..PurgeOptions::default()
        };
        let p = plan(css, &options, &[]);
        let out = p.purge(&page(&[], &["x"], &[], &["--d"]));
        assert!(
            out.starts_with(":root{--a:1px;--b:var(--a);--d:3px;--e:4px}"),
            "{out}"
        );
        assert!(!out.contains("--c"), "{out}");
        // Without the option every declaration stays.
        let p = plan(css, &PurgeOptions::default(), &[]);
        assert!(p.purge(&page(&[], &["x"], &[], &[])).contains("--c:2px"));
    }

    #[test]
    fn important_kept_or_dropped() {
        let css = ".a{color:red!important;margin:0}";
        let p = plan(css, &PurgeOptions::default(), &[]);
        assert_eq!(
            p.purge(&page(&[], &["a"], &[], &[])),
            ".a{margin:0;color:red!important}"
        );
        let options = PurgeOptions {
            drop_important: true,
            ..PurgeOptions::default()
        };
        let p = plan(css, &options, &[]);
        assert_eq!(
            p.purge(&page(&[], &["a"], &[], &[])),
            ".a{margin:0;color:red}"
        );
    }

    #[test]
    fn fallback_declarations_survive() {
        let css = ".i{background:radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%);\
                   background:-webkit-radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%)}";
        let p = plan(css, &PurgeOptions::default(), &[]);
        let out = p.purge(&page(&[], &["i"], &[], &[]));
        assert!(
            out.contains("background:radial-gradient(")
                && out.contains("background:-webkit-radial-gradient("),
            "{out}"
        );
    }

    #[test]
    fn byte_order_mark_is_dropped() {
        let p = plan("\u{feff}:root{--a:1}.b{x:1}", &PurgeOptions::default(), &[]);
        assert_eq!(p.purge(&page(&[], &[], &[], &[])), ":root{--a:1}");
    }

    #[test]
    fn rejected_rules_kept_as_written() {
        let p = plan(
            ".a{x:1}.b::before.c{content:\"x\"}@media (min-width:1px){.d{y:2}}",
            &PurgeOptions::default(),
            &[],
        );
        assert_eq!(
            p.purge(&page(&[], &["a"], &[], &[])),
            ".a{x:1}.b::before.c{content:\"x\"}"
        );
    }

    #[test]
    fn registry_placeholders() {
        let purges = CssPurges::default();
        let compile = || {
            PurgePlan::compile(
                ".a{x:1}.b{x:2}",
                &PurgeOptions::default(),
                &[],
                Targets::default(),
            )
        };
        let ph = purges.placeholder(7, compile).unwrap();
        assert_eq!(ph, "__nh_purge_0__");
        assert_eq!(purges.placeholder(7, || unreachable!()).unwrap(), ph);
        let html = format!("<style>{ph}</style><p class=b>");
        let out = purges
            .resolve(&html, || page(&[], &["b"], &[], &[]))
            .unwrap()
            .unwrap();
        assert_eq!(out, "<style>.b{x:2}</style><p class=b>");
        assert!(purges.resolve("<p>", PageNames::default).unwrap().is_none());
        assert!(
            purges
                .resolve("__nh_purge_9__", PageNames::default)
                .is_err()
        );
    }

    #[test]
    fn word_splitting() {
        let w: Vec<&str> = words("add('md:flex', \"w-1/2\"); --bs-x").collect();
        assert_eq!(
            w,
            [
                "add", "md:flex", "md", "flex", "w-1/2", "w-1", "2", "--bs-x"
            ]
        );
    }
}
