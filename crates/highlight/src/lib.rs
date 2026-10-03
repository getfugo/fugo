//! Code highlighting identical to the Go implementation's: Chroma's lexers, its HTML formatter
//! and styles, inside the Go implementation's wrappers (REWRITE_PLAN.md §2.1, decision D1).
//!
//! The Go implementation highlights with Chroma; sites style its output with Chroma class names
//! (`.chroma .k`) or rely on its inline styles. This crate is a port of Chroma v2.19.0: its
//! regex lexer engine and every lexer it ships (`chroma`: the XML lexers converted to Rust
//! data, the Go-written ones ported), on a port of the .NET regex dialect those lexers are
//! written in (`regexp2`); Chroma's HTML formatter (line structure, line numbers, highlighted
//! lines, classes or inline styles from Chroma's own style definitions) inside the Go
//! implementation's wrappers. Which lexer a language names is Chroma's decision
//! (`lexers.Get`), as in Go.
//!
//! [`Highlight`] implements [`ssg_markup::Highlighter`] for code fences and serves the
//! `highlight` template function ([`Highlight::highlight_with`]) and style sheets
//! ([`Highlight::css`]).

#![forbid(unsafe_code)]

mod chroma;
mod html;
mod lexers;
mod options;
mod regexp2;
mod style;
mod styles;
mod token;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock, PoisonError};

use ssg_base::Map;
use ssg_base::diag::Diagnostic;
use ssg_config::markup::HighlightConfig;
use ssg_markup::{HighlightOptions, Highlighter, HookError};

pub use lexers::Lexer;
pub use options::{CodeLayout, LineNumberLayout, Options, OptionsError, Styling};
pub use style::{CssMode, FALLBACK_STYLE};
pub use token::TokenType;

use crate::html::Block;
use crate::lexers::Languages;
use crate::style::{CssSettings, Style, Styles};

/// Why highlighting failed.
#[derive(Debug, thiserror::Error)]
pub enum HighlightError {
    /// Invalid options.
    #[error(transparent)]
    Options(#[from] OptionsError),
    /// A style sheet was asked for a style that does not exist.
    #[error("unknown highlighting style {0:?}")]
    UnknownStyle(String),
}

/// The options argument of the `highlight` template function.
#[derive(Clone, Copy, Debug)]
pub enum OptionsArg<'a> {
    /// No options: the site's defaults.
    None,
    /// An option string (`"linenos=table,hl_lines=2"`).
    Str(&'a str),
    /// An option map (`{"hl_inline": true, "noClasses": true}`).
    Map(&'a Map),
}

/// A token: its Chroma type and its text.
pub type Token = (TokenType, String);

/// Chroma's lexers and the bundled styles. They do not depend on the site, so they are loaded
/// once per process and shared (the server makes a `Highlight` for every rebuild); a lexer
/// compiles its rules the first time it is used.
struct Tables {
    languages: Languages,
    styles: Styles,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| Tables {
        languages: Languages::load(),
        styles: Styles::bundled(),
    })
}

/// The highlighter: Chroma's lexers and styles, and the site's defaults.
pub struct Highlight {
    languages: &'static Languages,
    styles: &'static Styles,
    defaults: Options,
    /// Style names that fell back to [`FALLBACK_STYLE`], for [`Highlight::diagnostics`].
    fallbacks: Mutex<BTreeSet<String>>,
}

impl std::fmt::Debug for Highlight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlight")
            .field("defaults", &self.defaults)
            .finish_non_exhaustive()
    }
}

impl Highlight {
    /// A highlighter with the site's `[markup.highlight]` settings. The first one in a process
    /// loads the lexer table; later ones share it.
    #[must_use]
    pub fn new(config: &HighlightConfig) -> Self {
        let tables = tables();
        Self {
            languages: &tables.languages,
            styles: &tables.styles,
            defaults: Options::from_config(config),
            fallbacks: Mutex::new(BTreeSet::new()),
        }
    }

    /// The site's default options.
    #[must_use]
    pub fn defaults(&self) -> &Options {
        &self.defaults
    }

    /// Whether `lang` names a language Chroma knows (Go's `transform.CanHighlight`).
    #[must_use]
    pub fn can_highlight(&self, lang: &str) -> bool {
        self.languages.get(lang).is_some()
    }

    /// The Chroma lexer `lang` names.
    #[must_use]
    pub fn lexer(&self, lang: &str) -> Option<Lexer<'static>> {
        self.languages.lexer(lang)
    }

    /// The names of every Chroma lexer, in Chroma's registration order.
    pub fn lexer_names(&self) -> impl Iterator<Item = &'static str> {
        self.languages.names()
    }

    /// The names of the bundled Chroma styles.
    pub fn style_names(&self) -> impl Iterator<Item = &str> {
        self.styles.names()
    }

    /// The `highlight` template function: `code` in `lang` with the site's defaults
    /// overridden by `opts`.
    ///
    /// # Errors
    /// Invalid options.
    pub fn highlight_with(
        &self,
        code: &str,
        lang: &str,
        opts: OptionsArg<'_>,
    ) -> Result<String, HighlightError> {
        let mut o = self.defaults.clone();
        match opts {
            OptionsArg::None => {}
            OptionsArg::Str(s) => o.apply_str(s)?,
            OptionsArg::Map(m) => o.apply_map(m)?,
        }
        Ok(self.highlight(code, lang, &o, None))
    }

    /// The HTML of `code` in `lang` with options `o`; `attributes` are a fence's attributes
    /// for the wrapping `<div>` (Go's `highlight`).
    #[must_use]
    pub fn highlight(
        &self,
        code: &str,
        lang: &str,
        o: &Options,
        attributes: Option<&Map>,
    ) -> String {
        let mut lang = lang.to_owned();
        let mut lexer = (!lang.is_empty())
            .then(|| self.languages.get(&lang))
            .flatten();
        if lexer.is_none() && o.guess_syntax {
            let guessed = self.languages.analyse(code);
            lang = guessed.name().to_lowercase();
            lexer = Some(guessed);
        }
        let Some(lexer) = lexer else {
            return html::plain(code, &lang, o.layout);
        };
        let tokens = lexer.tokens(code);
        let style = self.style(&o.style);
        let inline = match o.styling {
            Styling::Inline => style::inline_map(
                style,
                CssSettings {
                    all_classes: false,
                    tab_width: o.tab_width,
                    highlight_lines: !o.highlight_ranges().is_empty(),
                },
            ),
            Styling::Classes => BTreeMap::new(),
        };
        let tokens: Vec<(TokenType, &str)> =
            tokens.iter().map(|t| (t.ty, t.value.as_str())).collect();
        Block {
            lang: &lang,
            options: o,
            attributes,
            inline: &inline,
        }
        .format(&tokens)
    }

    /// The tokens of `code` in `lang` as Chroma's coalesced token stream (`None` when Chroma
    /// has no lexer for `lang`).
    #[must_use]
    pub fn tokens(&self, code: &str, lang: &str) -> Option<Vec<Token>> {
        let lexer = self.languages.get(lang)?;
        Some(
            lexer
                .tokens(code)
                .into_iter()
                .map(|t| (t.ty, t.value))
                .collect(),
        )
    }

    /// A style sheet for the Chroma style `style` (the Go program's `gen chromastyles`).
    ///
    /// # Errors
    /// An unknown style.
    pub fn css(&self, style: &str, mode: CssMode) -> Result<String, HighlightError> {
        let s = self
            .styles
            .get(&style.to_lowercase())
            .ok_or_else(|| HighlightError::UnknownStyle(style.to_owned()))?;
        Ok(style::stylesheet(s, mode))
    }

    /// Warnings so far: the style names that fell back to [`FALLBACK_STYLE`].
    #[must_use]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.fallbacks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|name| {
                Diagnostic::warning(format!(
                    "unknown highlighting style {name:?}; using {FALLBACK_STYLE:?}"
                ))
                .with_id("highlight-style")
            })
            .collect()
    }

    /// The style named `name` (exactly, as Chroma), else the fallback with a warning.
    fn style(&self, name: &str) -> &Style {
        self.styles.get(name).unwrap_or_else(|| {
            self.fallbacks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(name.to_owned());
            self.styles.fallback()
        })
    }
}

impl Highlighter for Highlight {
    /// A code fence no hook handled: the site's defaults, overridden by the fence's options;
    /// the fence's attributes go on the wrapping `<div>`; line ids default to `hl-<ordinal>`.
    fn highlight(&self, code: &str, lang: &str, o: &HighlightOptions) -> Result<String, HookError> {
        let mut opts = self.defaults.clone();
        opts.apply_map(&o.options).map_err(HookError::new)?;
        // Without `lineanchors`, line ids are numbered per code block, as in Go.
        if opts.line_anchors.is_empty() {
            opts.line_anchors = format!("hl-{}", o.ordinal);
        }
        // Go's code block renderer ends the code with a newline.
        let mut code = code.to_owned();
        if !code.is_empty() && !code.ends_with('\n') {
            code.push('\n');
        }
        Ok(Highlight::highlight(
            self,
            &code,
            lang,
            &opts,
            Some(&o.attributes),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighters_share_the_tables() {
        let a = Highlight::new(&HighlightConfig::default());
        let b = Highlight::new(&HighlightConfig::default());
        assert!(std::ptr::eq(a.languages, b.languages));
        assert!(std::ptr::eq(a.styles, b.styles));
        // Only the tables are shared: the fallback notices stay per highlighter.
        a.fallbacks.lock().unwrap().insert("x".into());
        assert!(b.fallbacks.lock().unwrap().is_empty());
    }
}
