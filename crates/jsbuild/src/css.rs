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

mod walk;
use walk::*;

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
mod tests;
