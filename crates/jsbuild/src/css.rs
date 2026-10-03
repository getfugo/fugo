//! CSS imported from `js.Build` scripts: the JS module esbuild 0.25.6 gives the importing
//! script, for rolldown's `load` hook (rolldown does not bundle CSS, and js.Build drops the CSS
//! file esbuild writes, so only what the script sees matters).
//!
//! - `css`: `export default {}`; the file is not parsed.
//! - `local-css` (and `global-css` with `:local(...)`): esbuild's CSS-module names. The default
//!   export maps each local name to its generated name, in order of first appearance; every
//!   name but `default` is also a named export (`"my-class"` too, as a string export name). A
//!   generated name is `<file>_<name>`, `<file>` being the file name without `.css` or
//!   `.module.css` made an identifier (`my-comp.module.css` → `my_comp`; `index.css` takes its
//!   directory's name). `composes` puts the composed names first, deduplicated.
//!
//! Local names are classes, ids, `@keyframes`, `@counter-style` and `@container` names, and the
//! names `animation(-name)`, `list-style(-type)`, `container(-name)` and `composes` refer to.
//! `:global(...)` keeps names global (`:local(...)` makes them local under `global-css`).
//! `composes: a from "./b.css"` imports `b.css` (loaded through this function again) and joins
//! the class strings when the module runs, deduplicating like esbuild.
//!
//! Parsing is lightningcss's (with its CSS-modules syntax). Where esbuild differs, rarely:
//! - a bare `:global`/`:local` (without parentheses) is an error, as in lightningcss;
//! - two files that would get the same name (`a/s.module.css` and `b/s.module.css` both with
//!   `.root`) are an error: esbuild suffixes one of them (`s_root2`), choosing by use counts and
//!   import order over the whole build, which a module-at-a-time load hook cannot reproduce;
//! - `minify` does not shorten names (esbuild's short names depend on the whole build);
//! - what lightningcss rejects loses its names where esbuild, which only warns, keeps them: an
//!   invalid selector drops its rule, an IE hack (`*zoom: 1`) the declarations before it;
//! - the names of a rule's `!important` declarations come after its other declarations';
//!   `composes` in a nested `& {}` and names in `@keyframes` blocks are ignored;
//! - `@import` and `url()` are not followed (esbuild fails on missing files), plain `css` is not
//!   checked for esbuild's two fatal errors (an unterminated comment, `\` before a newline);
//! - `composes` between two files that import each other sees an unfinished module, and a
//!   global name composed through two files appears once.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use lightningcss::css_modules::Config;
use lightningcss::error::{Error, ParserError, SelectorError};
use lightningcss::properties::Property;
use lightningcss::properties::animation::AnimationName;
use lightningcss::properties::contain::ContainerNameList;
use lightningcss::properties::css_modules::{Composes, Specifier};
use lightningcss::properties::custom::{Token, TokenOrValue};
use lightningcss::properties::list::{CounterStyle, ListStyleType};
use lightningcss::rules::keyframes::KeyframesName;
use lightningcss::rules::style::StyleRule;
use lightningcss::rules::{CssRule, CssRuleList, Location};
use lightningcss::selector::{Component, PseudoClass, Selector};
use lightningcss::stylesheet::{ParserOptions, StyleSheet};
use lightningcss::vendor_prefix::VendorPrefix;

use crate::lower::LowerError;

/// The esbuild loader of a CSS file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssLoader {
    Css,
    GlobalCss,
    LocalCss,
}

/// Per-build naming state: which file got each generated name.
#[derive(Debug, Default)]
pub struct CssNames {
    owners: HashMap<String, PathBuf>,
}

/// The JS module (ES module source) that a script's import of this CSS file sees under
/// `loader`. `filename` is the file's real path.
///
/// # Errors
/// CSS lightningcss cannot parse at all, a bare `:global`/`:local`, or a generated name another
/// file of the build already has.
pub fn css_module_js(
    loader: CssLoader,
    source: &str,
    filename: &Path,
    names: &mut CssNames,
) -> Result<String, LowerError> {
    let local = match loader {
        CssLoader::Css => return Ok(EMPTY_MODULE.to_owned()),
        // Only `:local(...)` makes names local here (esbuild's pseudo-class names are
        // case-sensitive).
        CssLoader::GlobalCss if !source.contains(":local") => {
            return Ok(EMPTY_MODULE.to_owned());
        }
        CssLoader::GlobalCss => false,
        CssLoader::LocalCss => true,
    };

    let warnings = Arc::new(RwLock::new(Vec::new()));
    let options = ParserOptions {
        filename: filename.to_string_lossy().into_owned(),
        // Parses `:local()`, `:global()` and `composes`; the names are generated below.
        css_modules: Some(Config::default()),
        error_recovery: true,
        warnings: Some(Arc::clone(&warnings)),
        ..ParserOptions::default()
    };
    let sheet = StyleSheet::parse(source, options).map_err(|e| parse_error(source, &e))?;
    // A rule with a bare `:global`/`:local` is dropped by lightningcss: its names would vanish.
    if let Ok(warnings) = warnings.read()
        && let Some(w) = warnings.iter().find(|w| {
            matches!(
                w.kind,
                ParserError::SelectorError(SelectorError::AmbiguousCssModuleClass(_))
            )
        })
    {
        let mut e = parse_error(source, w);
        e.message =
            "a bare \":global\" or \":local\" is not supported: wrap the selector, as in \":global(.name)\""
                .to_owned();
        return Err(e);
    }

    let mut walk = Walk {
        local,
        locals: Vec::new(),
        seen: HashSet::new(),
        composes: HashMap::new(),
    };
    walk.rules(&sheet.rules, false);

    let prefix = file_identifier(filename);
    for (name, loc) in &walk.locals {
        let generated = format!("{prefix}_{name}");
        if let Some(other) = names.owners.get(&generated).filter(|o| *o != filename) {
            let mut files = [other.display().to_string(), filename.display().to_string()];
            files.sort();
            let (line, column) = position(source, *loc);
            return Err(LowerError {
                message: format!(
                    "the CSS-module name {generated:?} would be generated for both {} and {}; \
                     esbuild renames one of them depending on the whole build, which is not \
                     supported: rename a file or the class",
                    files[0], files[1]
                ),
                line,
                column,
            });
        }
        names.owners.insert(generated, filename.to_owned());
    }
    Ok(walk.module_js(&prefix))
}

const EMPTY_MODULE: &str = "export default {};\n";

/// A name of the file: local (renamed) or global (kept).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Sym {
    name: String,
    local: bool,
}

/// The `composes` of one class: names of other files come first in its string.
#[derive(Debug, Default)]
struct Composed {
    imported: Vec<(String, String)>,
    names: Vec<Sym>,
}

/// One part of a class string: a name, or the export of another file's module.
enum Part {
    Name(String),
    Import(usize),
}

/// Collects the local names in esbuild's order: a style rule's selectors, then its nested
/// rules, then its declarations; an at-rule's name after its block.
struct Walk {
    /// Names are local unless `:global(...)` (`local-css`), else only in `:local(...)`.
    local: bool,
    /// Each local name with the rule it first appears in.
    locals: Vec<(String, Location)>,
    seen: HashSet<String>,
    composes: HashMap<Sym, Composed>,
}

impl Walk {
    fn name(&mut self, name: &str, local: bool, loc: Location) {
        if local && self.seen.insert(name.to_owned()) {
            self.locals.push((name.to_owned(), loc));
        }
    }

    /// `nested`: inside a style rule.
    fn rules(&mut self, rules: &CssRuleList<'_>, nested: bool) {
        for rule in &rules.0 {
            self.rule(rule, nested);
        }
    }

    fn rule(&mut self, rule: &CssRule<'_>, nested: bool) {
        match rule {
            CssRule::Style(style) => self.style(style, nested),
            CssRule::Media(r) => self.rules(&r.rules, nested),
            CssRule::Supports(r) => self.rules(&r.rules, nested),
            CssRule::LayerBlock(r) => self.rules(&r.rules, nested),
            CssRule::Scope(r) => self.rules(&r.rules, nested),
            CssRule::StartingStyle(r) => self.rules(&r.rules, nested),
            CssRule::MozDocument(r) => self.rules(&r.rules, nested),
            CssRule::Nesting(r) => self.style(&r.style, true),
            CssRule::Container(r) => {
                self.rules(&r.rules, nested);
                if let Some(name) = &r.name {
                    self.name(&name.0.0, self.local, r.loc);
                }
            }
            CssRule::Keyframes(r) => match &r.name {
                KeyframesName::Ident(name) => self.name(&name.0, self.local, r.loc),
                KeyframesName::Custom(name) => self.name(name, self.local, r.loc),
            },
            CssRule::CounterStyle(r) => self.name(&r.name.0, self.local, r.loc),
            _ => {}
        }
    }

    fn style(&mut self, rule: &StyleRule<'_>, nested: bool) {
        for selector in &rule.selectors.0 {
            self.selector(selector, self.local, rule.loc);
        }
        let block = &rule.declarations;
        let mut declarations: Vec<&Property<'_>> = block.declarations.iter().collect();
        declarations.extend(&block.important_declarations);
        for child in &rule.rules.0 {
            match child {
                // Declarations after nested rules still belong to this rule.
                CssRule::NestedDeclarations(r) => {
                    declarations.extend(&r.declarations.declarations);
                    declarations.extend(&r.declarations.important_declarations);
                }
                child => self.rule(child, true),
            }
        }
        // `composes` only works in a top-level rule of single class selectors.
        let parents = if nested {
            None
        } else {
            rule.selectors
                .0
                .iter()
                .map(|s| single_class(s, self.local))
                .collect::<Option<Vec<_>>>()
        };
        for property in declarations {
            self.property(property, parents.as_deref(), rule.loc);
        }
    }

    fn selector(&mut self, selector: &Selector<'_>, local: bool, loc: Location) {
        for component in source_order(selector) {
            match component {
                Component::Class(name) | Component::ID(name) => self.name(&name.0, local, loc),
                Component::NonTSPseudoClass(PseudoClass::Local { selector }) => {
                    self.selector(selector, true, loc);
                }
                Component::NonTSPseudoClass(PseudoClass::Global { selector }) => {
                    self.selector(selector, false, loc);
                }
                Component::Negation(list)
                | Component::Is(list)
                | Component::Where(list)
                | Component::Has(list) => {
                    for s in list {
                        self.selector(s, local, loc);
                    }
                }
                Component::NthOf(nth) => {
                    for s in nth.selectors() {
                        self.selector(s, local, loc);
                    }
                }
                // esbuild leaves the arguments of other pseudo-classes and elements alone.
                _ => {}
            }
        }
    }

    fn property(&mut self, property: &Property<'_>, parents: Option<&[Sym]>, loc: Location) {
        match property {
            Property::Composes(composes) => {
                if let Some(parents) = parents {
                    self.composes(composes, parents, loc);
                }
            }
            Property::Animation(list, prefix) if *prefix == VendorPrefix::None => {
                for animation in list {
                    self.animation_name(&animation.name, loc);
                }
            }
            Property::AnimationName(list, prefix) if *prefix == VendorPrefix::None => {
                for name in list {
                    self.animation_name(name, loc);
                }
            }
            Property::ListStyle(list) => self.counter_style(&list.list_style_type, loc),
            Property::ListStyleType(list) => self.counter_style(list, loc),
            Property::Container(container) => self.container_names(&container.name, loc),
            Property::ContainerName(names) => self.container_names(names, loc),
            Property::Unparsed(unparsed) if unparsed.property_id.prefix() == VendorPrefix::None => {
                // `animation: spin var(--time)`: esbuild's scan of the shorthand's tokens.
                let id = unparsed.property_id.name();
                if id == "animation" || id == "animation-name" {
                    self.animation_tokens(&unparsed.value.0, id == "animation", loc);
                }
            }
            _ => {}
        }
    }

    fn animation_name(&mut self, name: &AnimationName<'_>, loc: Location) {
        match name {
            AnimationName::Ident(name) if !is_keyword(&name.0) => {
                self.name(&name.0, self.local, loc);
            }
            AnimationName::String(name) => self.name(&name.0, self.local, loc),
            _ => {}
        }
    }

    /// esbuild's `processAnimationShorthand` (`processAnimationName` when not `shorthand`):
    /// the first identifier of each comma-separated animation that is not a keyword of
    /// another longhand is its name.
    fn animation_tokens(&mut self, tokens: &[TokenOrValue<'_>], shorthand: bool, loc: Location) {
        let mut found = [false; 6]; // timing, iteration count, direction, fill, play state, name
        for token in tokens {
            match token {
                TokenOrValue::Token(Token::Comma) => found = [false; 6],
                TokenOrValue::Token(Token::Number { .. }) if shorthand => found[1] = true,
                TokenOrValue::Token(Token::String(name)) if !shorthand || !found[5] => {
                    found[5] = true;
                    self.name(name, self.local, loc);
                }
                TokenOrValue::Token(Token::Ident(ident)) => {
                    if !shorthand {
                        if !is_keyword(ident) {
                            self.name(ident, self.local, loc);
                        }
                        continue;
                    }
                    let lower = ident.to_ascii_lowercase();
                    let slot = match lower.as_str() {
                        "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out"
                        | "step-start" | "step-end"
                            if !found[0] =>
                        {
                            0
                        }
                        "infinite" if !found[1] => 1,
                        "normal" | "reverse" | "alternate" | "alternate-reverse" if !found[2] => 2,
                        "none" | "forwards" | "backwards" | "both" if !found[3] => 3,
                        "running" | "paused" if !found[4] => 4,
                        _ if !found[5] => {
                            if !is_keyword(ident) {
                                self.name(ident, self.local, loc);
                            }
                            5
                        }
                        _ => continue,
                    };
                    found[slot] = true;
                }
                _ => {}
            }
        }
    }

    fn counter_style(&mut self, list: &ListStyleType<'_>, loc: Location) {
        if let ListStyleType::CounterStyle(CounterStyle::Name(name)) = list
            && !is_keyword(&name.0)
        {
            self.name(&name.0, self.local, loc);
        }
    }

    fn container_names(&mut self, names: &ContainerNameList<'_>, loc: Location) {
        if let ContainerNameList::Names(names) = names {
            for name in names {
                if !is_keyword(&name.0.0) {
                    self.name(&name.0.0, self.local, loc);
                }
            }
        }
    }

    fn composes(&mut self, composes: &Composes<'_>, parents: &[Sym], loc: Location) {
        for parent in parents {
            for name in &composes.names {
                let name = name.0.to_string();
                let entry = self.composes.entry(parent.clone()).or_default();
                match &composes.from {
                    None => {
                        entry.names.push(Sym {
                            name: name.clone(),
                            local: self.local,
                        });
                        self.name(&name, self.local, loc);
                    }
                    Some(Specifier::Global) => entry.names.push(Sym { name, local: false }),
                    // Like esbuild, `composes` from an external URL is ignored.
                    Some(Specifier::File(file)) if !is_external(file) => {
                        entry.imported.push((file.to_string(), name));
                    }
                    Some(_) => {}
                }
            }
        }
    }

    /// What `root` composes, before its own name: esbuild's depth-first walk of `composes`,
    /// each name once, names of other files first at each class. Other files' strings come
    /// from their modules (indexes into `imports`).
    fn composed(&self, root: &Sym, prefix: &str, imports: &mut Vec<(String, String)>) -> Vec<Part> {
        let mut visited = HashSet::from([root.clone()]);
        let mut parts = Vec::new();
        self.visit(root, prefix, imports, &mut visited, &mut parts);
        parts
    }

    fn visit(
        &self,
        sym: &Sym,
        prefix: &str,
        imports: &mut Vec<(String, String)>,
        visited: &mut HashSet<Sym>,
        parts: &mut Vec<Part>,
    ) {
        let Some(composed) = self.composes.get(sym) else {
            return;
        };
        for import in &composed.imported {
            let index = match imports.iter().position(|i| i == import) {
                Some(i) => i,
                None => {
                    imports.push(import.clone());
                    imports.len() - 1
                }
            };
            parts.push(Part::Import(index));
        }
        for name in &composed.names {
            if visited.insert(name.clone()) {
                self.visit(name, prefix, imports, visited, parts);
                parts.push(Part::Name(generated(name, prefix)));
            }
        }
    }

    /// The ES module: one variable per local name, the default export object, named exports.
    fn module_js(&self, prefix: &str) -> String {
        if self.locals.is_empty() {
            return EMPTY_MODULE.to_owned();
        }
        let mut imports = Vec::new();
        let mut vars = String::new();
        let mut needs_join = false;
        for (i, (name, _)) in self.locals.iter().enumerate() {
            let sym = Sym {
                name: name.clone(),
                local: true,
            };
            let own = generated(&sym, prefix);
            let parts = self.composed(&sym, prefix, &mut imports);
            let value = if parts.iter().any(|p| matches!(p, Part::Import(_))) {
                // Deduplicated when the module runs, against the other modules' strings.
                needs_join = true;
                let parts: Vec<String> = parts.iter().map(part_js).collect();
                format!("__ssg_compose({}, [{}])", js_string(&own), parts.join(", "))
            } else {
                let mut names: Vec<&str> = parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Name(n) => Some(n.as_str()),
                        Part::Import(_) => None,
                    })
                    .collect();
                names.push(&own);
                js_string(&names.join(" "))
            };
            vars.push_str(&format!("var c{i} = {value};\n"));
        }

        let mut js = String::new();
        for (i, (file, name)) in imports.iter().enumerate() {
            js.push_str(&format!(
                "import {{ {} as i{i} }} from {};\n",
                export_name(name),
                js_string(file)
            ));
        }
        js.push_str(&vars);
        let entries: Vec<String> = self
            .locals
            .iter()
            .enumerate()
            .map(|(i, (name, _))| format!("{}: c{i}", js_string(name)))
            .collect();
        js.push_str(&format!("export default {{ {} }};\n", entries.join(", ")));
        let named: Vec<String> = self
            .locals
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| name != "default")
            .map(|(i, (name, _))| format!("c{i} as {}", export_name(name)))
            .collect();
        if !named.is_empty() {
            js.push_str(&format!("export {{ {} }};\n", named.join(", ")));
        }
        if needs_join {
            js.push_str(COMPOSE_JS);
        }
        js
    }
}

/// Joins class strings like esbuild's `composes`: each name once, `own` last.
const COMPOSE_JS: &str = r#"function __ssg_compose(own, parts) {
  var seen = Object.create(null), out = [], i, j, names;
  seen[own] = true;
  for (i = 0; i < parts.length; i++) {
    names = typeof parts[i] === "string" ? parts[i].split(" ") : [];
    for (j = 0; j < names.length; j++) {
      if (names[j] && !seen[names[j]]) {
        seen[names[j]] = true;
        out.push(names[j]);
      }
    }
  }
  out.push(own);
  return out.join(" ");
}
"#;

fn part_js(part: &Part) -> String {
    match part {
        Part::Name(name) => js_string(name),
        Part::Import(i) => format!("i{i}"),
    }
}

fn generated(sym: &Sym, prefix: &str) -> String {
    if sym.local {
        format!("{prefix}_{}", sym.name)
    } else {
        sym.name.clone()
    }
}

/// The class of a selector that is a single class (`.a`, `:local(.a)`), if it is one.
fn single_class(selector: &Selector<'_>, local: bool) -> Option<Sym> {
    match source_order(selector).as_slice() {
        [Component::Class(name)] => Some(Sym {
            name: name.0.to_string(),
            local,
        }),
        [Component::NonTSPseudoClass(PseudoClass::Local { selector })] => {
            single_class(selector, true)
        }
        [Component::NonTSPseudoClass(PseudoClass::Global { selector })] => {
            single_class(selector, false)
        }
        _ => None,
    }
}

/// A selector's simple selectors in source order, without combinators (parcel_selectors stores
/// the compounds right to left).
fn source_order<'a, 'i>(selector: &'a Selector<'i>) -> Vec<&'a Component<'i>> {
    let mut compounds: Vec<Vec<&Component<'i>>> = vec![Vec::new()];
    for component in selector.iter_raw_match_order() {
        match component {
            Component::Combinator(_) => compounds.push(Vec::new()),
            c => {
                if let Some(last) = compounds.last_mut() {
                    last.push(c);
                }
            }
        }
    }
    compounds.into_iter().rev().flatten().collect()
}

/// `none` and the CSS-wide keywords, which esbuild never takes for names.
fn is_keyword(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "none" | "initial" | "inherit" | "unset" | "default" | "revert" | "revert-layer"
    )
}

/// URLs esbuild leaves external.
fn is_external(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://") || path.starts_with("//")
}

/// esbuild's identifier for a file (`GenerateNonUniqueNameFromPath`): the file name without
/// its extension (`.module.css` counts as one), or the directory's name for `index`, with
/// every run of other characters than ASCII letters (and digits, after the first letter)
/// made one `_`.
fn file_identifier(path: &Path) -> String {
    let stem = |p: &Path| {
        let base = p
            .file_name()
            .map(|b| b.to_string_lossy().into_owned())
            .unwrap_or_default();
        match base.strip_suffix(".module.css") {
            Some(s) if !s.is_empty() => s.to_owned(),
            _ => match base.rfind('.') {
                Some(dot) => base[..dot].to_owned(),
                None => base,
            },
        }
    };
    let mut base = stem(path);
    if base == "index"
        && let Some(dir) = path.parent().map(stem).filter(|d| !d.is_empty())
    {
        base = dir;
    }
    let mut out = String::new();
    let mut gap = false;
    for c in base.chars() {
        if c.is_ascii_alphabetic() || (!out.is_empty() && c.is_ascii_digit()) {
            if gap {
                out.push('_');
                gap = false;
            }
            out.push(c);
        } else if !out.is_empty() {
            gap = true;
        }
    }
    if out.is_empty() { "_".to_owned() } else { out }
}

/// A JS string literal (ES2015: U+2028 and U+2029 escaped too).
fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' | '\u{2029}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// An export or import name: an ASCII identifier as is, anything else as a string (ES2022's
/// arbitrary module namespace names, which rolldown resolves while bundling).
fn export_name(name: &str) -> String {
    let mut chars = name.chars();
    let ident = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if ident {
        name.to_owned()
    } else {
        js_string(name)
    }
}

/// A lightningcss error with esbuild's position convention.
fn parse_error(source: &str, e: &Error<ParserError<'_>>) -> LowerError {
    let (line, column) = e.loc.as_ref().map_or((1, 0), |l| {
        position(
            source,
            Location {
                source_index: 0,
                line: l.line,
                column: l.column,
            },
        )
    });
    LowerError {
        message: e.kind.to_string(),
        line,
        column,
    }
}

/// The 1-based line and 0-based byte column of a lightningcss location (0-based line,
/// 1-based column in UTF-16 code units; lines end at `\n`, `\r\n`, `\r` or a form feed).
fn position(source: &str, loc: Location) -> (u32, u32) {
    let mut line = 0;
    let mut start = 0;
    let bytes = source.as_bytes();
    let mut i = 0;
    while line < loc.line && i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => i += 1,
            b'\n' | b'\r' | 0x0C => {}
            _ => {
                i += 1;
                continue;
            }
        }
        i += 1;
        line += 1;
        start = i;
    }
    let mut units = 1;
    let mut column = 0;
    for c in source[start..].chars() {
        if units >= loc.column as usize || matches!(c, '\n' | '\r' | '\x0C') {
            break;
        }
        units += c.len_utf16();
        column += c.len_utf8();
    }
    (loc.line + 1, column as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn js(loader: CssLoader, source: &str) -> String {
        css_module_js(
            loader,
            source,
            Path::new("/site/assets/a.module.css"),
            &mut CssNames::default(),
        )
        .expect("css_module_js")
    }

    fn err(source: &str) -> LowerError {
        css_module_js(
            CssLoader::LocalCss,
            source,
            Path::new("/site/assets/a.module.css"),
            &mut CssNames::default(),
        )
        .expect_err("an error")
    }

    #[test]
    fn plain_css_is_an_empty_object() {
        assert_eq!(
            js(CssLoader::Css, ".a { composes: b } :local(.c) {}"),
            EMPTY_MODULE
        );
        assert_eq!(js(CssLoader::GlobalCss, ".a {} #b {}"), EMPTY_MODULE);
        assert_eq!(js(CssLoader::LocalCss, ":global(.a) {}"), EMPTY_MODULE);
    }

    #[test]
    fn local_names_in_order_of_appearance() {
        let out = js(
            CssLoader::LocalCss,
            ".btn:hover .icon {} #main {} .my-class {} .default {} @keyframes spin {}",
        );
        assert_eq!(
            out,
            "var c0 = \"a_btn\";\nvar c1 = \"a_icon\";\nvar c2 = \"a_main\";\n\
             var c3 = \"a_my-class\";\nvar c4 = \"a_default\";\nvar c5 = \"a_spin\";\n\
             export default { \"btn\": c0, \"icon\": c1, \"main\": c2, \"my-class\": c3, \
             \"default\": c4, \"spin\": c5 };\n\
             export { c0 as btn, c1 as icon, c2 as main, c3 as \"my-class\", c5 as spin };\n"
        );
    }

    #[test]
    fn global_and_local() {
        let out = js(
            CssLoader::LocalCss,
            ":global(.g) .l {} :global(.h .i) {} .j:not(:global(.k)) {}",
        );
        assert!(
            out.contains("export default { \"l\": c0, \"j\": c1 };"),
            "{out}"
        );
        let out = js(
            CssLoader::GlobalCss,
            ".g :local(.l) { animation: k } :local(#m) {}",
        );
        assert!(
            out.contains("export default { \"l\": c0, \"m\": c1 };"),
            "{out}"
        );
    }

    #[test]
    fn keyframes_and_animations() {
        let out = js(
            CssLoader::LocalCss,
            ".a { animation: fade 1s ease-in infinite, none 2s } @keyframes \"slide\" {} \
             .b { animation-name: inherit, grow; -webkit-animation: prefixed 1s } \
             .c { animation: spin var(--t) linear }",
        );
        assert!(
            out.contains(
                "export default { \"a\": c0, \"fade\": c1, \"slide\": c2, \"b\": c3, \
                 \"grow\": c4, \"c\": c5, \"spin\": c6 };"
            ),
            "{out}"
        );
    }

    #[test]
    fn composes_in_the_same_file() {
        let out = js(
            CssLoader::LocalCss,
            ".a { composes: b c } .b { composes: c } .c {} .d { composes: g from global } \
             .e .f { composes: c }",
        );
        assert!(out.contains("var c0 = \"a_c a_b a_a\";"), "{out}");
        assert!(out.contains("var c1 = \"a_c a_b\";"), "{out}");
        assert!(out.contains("var c3 = \"g a_d\";"), "{out}");
        // Not a single class selector: ignored, like esbuild (which warns).
        assert!(out.contains("var c5 = \"a_f\";"), "{out}");
    }

    #[test]
    fn composes_from_another_file() {
        let out = js(
            CssLoader::LocalCss,
            ".a { composes: x my-y from \"./b.module.css\"; composes: c } .c {} \
             .d { composes: z from 'https://example.com/c.css' }",
        );
        assert!(out.starts_with(
            "import { x as i0 } from \"./b.module.css\";\n\
             import { \"my-y\" as i1 } from \"./b.module.css\";\n"
        ));
        assert!(
            out.contains("var c0 = __ssg_compose(\"a_a\", [i0, i1, \"a_c\"]);"),
            "{out}"
        );
        assert!(out.contains("var c2 = \"a_d\";"), "{out}");
        assert!(out.contains("function __ssg_compose(own, parts)"), "{out}");
    }

    #[test]
    fn file_identifiers() {
        for (path, ident) in [
            ("/x/a.module.css", "a"),
            ("/x/my-comp.module.css", "my_comp"),
            ("/x/1x.css", "x"),
            ("/x/a.b.c.css", "a_b_c"),
            ("/x/a.module.module.css", "a_module"),
            ("/x/comp/index.module.css", "comp"),
            ("/x/v1.2/index.css", "v1"),
            ("/x/é.css", "_"),
            ("/x/$d.css", "d"),
        ] {
            assert_eq!(file_identifier(Path::new(path)), ident, "{path}");
        }
    }

    #[test]
    fn names_are_unique_per_build() {
        let mut names = CssNames::default();
        let a = Path::new("/site/x/s.module.css");
        let b = Path::new("/site/y/s.module.css");
        css_module_js(CssLoader::LocalCss, ".root {}", a, &mut names).expect("first");
        css_module_js(CssLoader::LocalCss, ".root {}", a, &mut names).expect("same file again");
        css_module_js(CssLoader::LocalCss, ".other {}", b, &mut names).expect("other names");
        let e = css_module_js(CssLoader::LocalCss, "\n  .other, .root {}", b, &mut names)
            .expect_err("collision");
        assert_eq!((e.line, e.column), (2, 2));
        assert!(e.message.starts_with(
            "the CSS-module name \"s_root\" would be generated for both /site/x/s.module.css \
             and /site/y/s.module.css"
        ));
    }

    #[test]
    fn bare_global_is_an_error_with_its_position() {
        let e = err(".a {}\n.日本 :global .c {}");
        assert_eq!((e.line, e.column), (2, 9));
        assert!(e.message.starts_with("a bare \":global\""), "{e:?}");
    }

    #[test]
    fn strings_and_export_names() {
        assert_eq!(
            js_string("a\"b\\c\n\u{2028}\u{1}"),
            "\"a\\\"b\\\\c\\n\\u2028\\x01\""
        );
        assert_eq!(export_name("btn"), "btn");
        assert_eq!(export_name("class"), "class");
        assert_eq!(export_name("my-x"), "\"my-x\"");
        assert_eq!(export_name("日本"), "\"日本\"");
    }
}
