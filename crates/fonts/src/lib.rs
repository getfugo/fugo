//! `[fonts]`: the published fonts cut down to the characters the site uses.
//!
//! The build writes its files through a [`Recorder`], which notes the characters of every HTML
//! and CSS file and the paths of the fonts ([`css`] and [`chars`] say which characters count). After the
//! last file, [`subset_fonts`] cuts each font a `[[fonts.subset]]` entry covers down to the
//! characters that entry asks for ([`subset::cut`]), in the font's own format, and writes it
//! back. A font none of whose characters are used is left as it is.

mod chars;
mod config;
mod css;
mod record;
mod subset;
mod woff;

use rayon::prelude::*;
use ssg_base::diag::Diagnostic;
use ssg_base::paths::OutputPath;
use ssg_config::Config;

pub use chars::add_html_text;
pub use config::{FontsConfig, Rule, Source};
pub use css::CssStrings;
pub use record::Recorder;
pub use subset::{Format, Outcome, SubsetError, cut};
pub use woff::encode as encode_woff;

/// What went wrong.
#[derive(Debug, thiserror::Error)]
pub enum FontsError {
    #[error("{key}: {message}")]
    Config { key: String, message: String },
    #[error("{path}: {source}")]
    Font {
        path: OutputPath,
        source: SubsetError,
    },
    #[error("{path}: {source}")]
    Io {
        path: OutputPath,
        source: std::io::Error,
    },
}

impl FontsError {
    pub(crate) fn config(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Config {
            key: key.into(),
            message: message.into(),
        }
    }
}

/// The configuration's `[fonts]`, checked (`None` without `[[fonts.subset]]` entries).
///
/// # Errors
/// See [`FontsConfig::from_tree`].
pub fn settings(cfg: &Config) -> Result<Option<FontsConfig>, FontsError> {
    FontsConfig::from_tree(cfg.raw.get("fonts"))
}

/// A font the build cut down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cut {
    pub path: OutputPath,
    /// Its size before and after, in bytes.
    pub before: usize,
    pub after: usize,
}

/// What [`subset_fonts`] did.
#[derive(Debug, Default)]
pub struct Subsetted {
    /// The fonts cut down, by path.
    pub cuts: Vec<Cut>,
    /// Entries that cover no font, and fonts left whole on purpose.
    pub warnings: Vec<Diagnostic>,
}

/// Cuts down the fonts the entries of `config` cover, among those the recorder saw and
/// `published` (the static files, which are copied around the recorder: their HTML and CSS are
/// noted first), and writes them into the recorder's sink. A font covered by several entries
/// follows the first.
///
/// # Errors
/// A file that cannot be read, or a font that cannot be cut down or written.
pub fn subset_fonts(
    config: &FontsConfig,
    recorder: &Recorder,
    published: &[OutputPath],
) -> Result<Subsetted, FontsError> {
    published
        .par_iter()
        .filter(|p| record::is_noted(p))
        .try_for_each(|path| {
            let bytes = recorder
                .inner()
                .read(path)
                .map_err(|source| FontsError::Io {
                    path: path.clone(),
                    source,
                })?;
            recorder.note(path, &bytes);
            Ok::<(), FontsError>(())
        })?;
    let mut fonts = recorder.fonts();
    fonts.extend(published.iter().filter(|p| record::is_font(p)).cloned());
    fonts.sort();
    fonts.dedup();

    let mut out = Subsetted::default();
    let css = recorder.css_chars();
    let text = config.needs_text().then(|| recorder.text_chars());
    let mut jobs = Vec::new();
    let mut chars = Vec::with_capacity(config.rules.len());
    for (i, rule) in config.rules.iter().enumerate() {
        let mut used = match rule.from {
            Source::Content => css.clone(),
            Source::Text => text.clone().unwrap_or_default(),
        };
        used.extend(rule.keep.chars());
        chars.push(used);
        let mut matched = false;
        for path in &fonts {
            if rule.matches(path.as_str()) {
                matched = true;
                if !jobs.iter().any(|(p, _)| p == path) {
                    jobs.push((path.clone(), i));
                }
            }
        }
        if !matched {
            out.warnings.push(
                Diagnostic::warning(format!(
                    "fonts.subset[{i}]: no published font matches {}",
                    rule.patterns.join(", ")
                ))
                .with_id("fonts-no-match"),
            );
        }
    }
    let sink = recorder.inner();
    let results: Vec<Result<Done, FontsError>> = jobs
        .par_iter()
        .map(|(path, rule)| subset_one(sink, path, &chars[*rule]))
        .collect();
    for r in results {
        match r? {
            Done::Cut(c) => out.cuts.push(c),
            Done::Warning(w) => out.warnings.push(w),
            Done::Nothing => {}
        }
    }
    out.cuts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// What became of one font.
enum Done {
    Cut(Cut),
    /// Left whole, with the reason.
    Warning(Diagnostic),
    Nothing,
}

/// Cuts the font at `path` in `sink` down to `chars`, in place.
fn subset_one(
    sink: &dyn ssg_base::Sink,
    path: &OutputPath,
    chars: &std::collections::BTreeSet<char>,
) -> Result<Done, FontsError> {
    let io = |source| FontsError::Io {
        path: path.clone(),
        source,
    };
    let bytes = sink.read(path).map_err(io)?;
    let outcome = cut(&bytes, chars).map_err(|source| FontsError::Font {
        path: path.clone(),
        source,
    })?;
    Ok(match outcome {
        Outcome::Cut(cut) => {
            sink.write(path, &cut).map_err(io)?;
            Done::Cut(Cut {
                path: path.clone(),
                before: bytes.len(),
                after: cut.len(),
            })
        }
        Outcome::Whole(Some(reason)) => Done::Warning(
            Diagnostic::warning(format!("{path}: left whole: {reason}"))
                .with_id("fonts-left-whole"),
        ),
        Outcome::Whole(None) | Outcome::Unused => Done::Nothing,
    })
}
