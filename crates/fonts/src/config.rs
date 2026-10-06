//! `[fonts]`: which published fonts are cut down, and to which characters.
//!
//! ```toml
//! [[fonts.subset]]
//! paths = ["assets/webfonts/*"]  # globs of the fonts' paths in the published site
//! from = "content"               # "text" (the default) or "content"
//! keep = "…"                     # characters kept anyway
//! ```

use serde::Deserialize;
use ssg_base::Value;
use ssg_base::glob::{self, Glob, GlobOpts};

use crate::FontsError;

/// The decoded `[fonts]` table.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct Raw {
    subset: Vec<RawRule>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct RawRule {
    paths: Vec<String>,
    from: Option<String>,
    keep: String,
}

/// Which characters of the site count as used by a font.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    /// The pages' text, and the strings of the CSS `content` declarations (text fonts).
    #[default]
    Text,
    /// Only the strings of the CSS `content` declarations (icon fonts, whose characters only
    /// style sheets name: `.fa-star::before { content: "\f005" }`).
    Content,
}

impl Source {
    const NAMES: &[&str] = &["text", "content"];

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "text" => Some(Self::Text),
            "content" => Some(Self::Content),
            _ => None,
        }
    }
}

/// One `[[fonts.subset]]` entry.
#[derive(Debug)]
pub struct Rule {
    /// The patterns as written, for messages.
    pub patterns: Vec<String>,
    globs: Vec<Glob>,
    pub from: Source,
    /// Characters kept whether the site uses them or not.
    pub keep: String,
}

impl Rule {
    /// Whether the rule covers the published file at `path` (no leading slash).
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        let path = path.trim_start_matches('/');
        self.globs.iter().any(|g| g.is_match(path))
    }
}

/// The checked `[fonts]` settings.
#[derive(Debug)]
pub struct FontsConfig {
    pub rules: Vec<Rule>,
}

impl FontsConfig {
    /// Decodes and checks `[fonts]`; `None` without one, or without `subset` entries.
    ///
    /// # Errors
    /// An unknown key, a wrong type, an entry without paths, a bad glob or an unknown `from`.
    pub fn from_tree(fonts: Option<&Value>) -> Result<Option<Self>, FontsError> {
        let Some(v) = fonts else {
            return Ok(None);
        };
        let raw: Raw = ssg_config::de::from_value(v).map_err(|e| {
            let path = e.dotted_path();
            FontsError::config(
                if path.is_empty() {
                    "fonts".to_owned()
                } else {
                    format!("fonts.{path}")
                },
                e.message,
            )
        })?;
        if raw.subset.is_empty() {
            return Ok(None);
        }
        let mut rules = Vec::with_capacity(raw.subset.len());
        for (i, r) in raw.subset.into_iter().enumerate() {
            let key = |k: &str| format!("fonts.subset[{i}].{k}");
            if r.paths.is_empty() {
                return Err(FontsError::config(key("paths"), "no paths"));
            }
            let globs = r
                .paths
                .iter()
                .map(|p| {
                    glob::compile(p.trim_start_matches('/'), GlobOpts::default())
                        .map_err(|e| FontsError::config(key("paths"), format!("{p:?}: {e}")))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let from = match r.from.as_deref() {
                None => Source::default(),
                Some(s) => Source::parse(s).ok_or_else(|| {
                    FontsError::config(
                        key("from"),
                        format!("{s:?} is not one of {}", Source::NAMES.join(", ")),
                    )
                })?,
            };
            rules.push(Rule {
                patterns: r.paths,
                globs,
                from,
                keep: r.keep,
            });
        }
        Ok(Some(Self { rules }))
    }

    /// Whether a rule needs the pages' text (not only their CSS `content` strings).
    #[must_use]
    pub fn needs_text(&self) -> bool {
        self.rules.iter().any(|r| r.from == Source::Text)
    }
}
