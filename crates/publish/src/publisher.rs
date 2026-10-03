//! The publisher: every rendered output goes through [`Publisher::emit`].
//!
//! The steps of `emit` (REWRITE_PLAN.md §3.4):
//! 0. `purge_css` placeholders (`__nh_purge_<n>__`) are replaced by the CSS this output uses
//!    ([`ssg_minify::CssPurges`]): the tags, classes and ids of its elements, the words of
//!    its scripts and the custom properties it mentions ([`page_names`]);
//! 1. `canonifyURLs` / `relativeURLs` rewrite (always for RSS, for HTML outputs when
//!    configured; a held output is rewritten again after [`Publisher::patch_held`] inserted its
//!    replacements, so post-processed links follow the site's URL style);
//! 2. the LiveReload script (`serve` only; HTML outputs that are not alias redirects, as in
//!    Go);
//! 3. URL-token extraction;
//! 4. an output holding a deferred placeholder (`__nh_defer_<key>__`, `__nh_pp_<id>_<field>__`)
//!    is held until [`Publisher::patch_held`]: its text is written to the sink as it is (to its
//!    file in a disk build, as Go's post-processing does, so held pages take no memory however
//!    many there are) and only its path is kept; `patch_held` reads it back, patches, minifies
//!    and writes it again. Any other output is minified (when `minifyOutput` is set and the
//!    media type has a minifier) and written.
//!
//! An empty output writes no file. A minifier failure writes the output unminified and records
//! a warning.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use aho_corasick::{AhoCorasick, MatchKind};
use rayon::prelude::*;
use ssg_base::diag::{Diagnostic, Diagnostics};
use ssg_base::paths::OutputPath;
use ssg_base::url::{LinkStyle, UrlRef};
use ssg_base::{FormatId, IdVec, LangIdx, MediaTypeId, Sink};
use ssg_config::site::LinkOutput;
use ssg_config::{Config, MediaTypes, OutputFormats};
use ssg_minify::purge::{self, PageNames};
use ssg_minify::{CssPurges, Minifier};

use crate::canonify::{Quoting, UrlRewriter};
use crate::elements::HtmlElements;
use crate::tokens::UrlTokens;
use crate::{PublishError, livereload};

/// The prefixes of the placeholders that hold an output back until the deferred wave.
pub const PLACEHOLDER_PREFIXES: [&str; 2] = ["__nh_defer_", "__nh_pp_"];

/// A rendered output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub path: OutputPath,
    pub text: String,
    pub format: FormatId,
    /// The language whose base URL canonifies the output.
    pub lang: LangIdx,
    /// An alias redirect (front matter alias, `page/1/` alias or language redirect): Go
    /// publishes these without the LiveReload script.
    pub alias: bool,
}

/// How one language writes URLs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteLinks {
    /// The base URL, with a trailing slash.
    pub base_url: String,
    /// `canonifyURLs`.
    pub style: LinkStyle,
    /// `relativeURLs`.
    pub output: LinkOutput,
    /// The LiveReload server whose script goes into this language's HTML pages (`serve`; the
    /// URL's path and port are used). `None`: no script.
    pub livereload: Option<UrlRef>,
}

impl SiteLinks {
    /// The rewriter of an output at `target`: RSS is always rewritten, HTML only with
    /// `canonifyURLs` or `relativeURLs`.
    fn rewriter(&self, target: &OutputPath, is_html: bool, is_rss: bool) -> Option<UrlRewriter> {
        let configured = self.style == LinkStyle::Canonify || self.output == LinkOutput::Relative;
        if !(is_rss || is_html && configured) {
            return None;
        }
        Some(if self.output == LinkOutput::Relative {
            UrlRewriter::relative(target)
        } else {
            UrlRewriter::absolute(&self.base_url)
        })
    }
}

/// What the publisher does.
#[derive(Clone, Debug)]
pub struct PublishSettings {
    pub sites: IdVec<LangIdx, SiteLinks>,
    pub formats: Arc<OutputFormats>,
    pub media_types: Arc<MediaTypes>,
    /// `Some` with `minifyOutput`.
    pub minifier: Option<Minifier>,
}

impl PublishSettings {
    /// The settings of a configuration (no LiveReload).
    ///
    /// # Errors
    /// An invalid `[minify.tdewolff]` option or browserslist configuration.
    pub fn from_config(cfg: &Config) -> Result<Self, PublishError> {
        let mut sites = IdVec::with_capacity(cfg.sites.len());
        for s in &cfg.sites {
            sites.push(SiteLinks {
                base_url: s.base_url.as_str().to_owned(),
                style: s.urls.link_style,
                output: s.urls.output,
                livereload: None,
            });
        }
        let minifier = if cfg.minify.minify_output {
            let browsers = ssg_minify::project_browsers(&cfg.project_dir, &cfg.environment)?;
            Some(Minifier::new(&cfg.minify)?.with_browsers(browsers))
        } else {
            None
        };
        Ok(Self {
            sites,
            formats: Arc::clone(&cfg.output_formats),
            media_types: Arc::clone(&cfg.media_types),
            minifier,
        })
    }
}

/// What [`Publisher::emit`] did with an output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emitted {
    Written,
    /// Held for the deferred wave.
    Held,
    /// Empty: no file.
    Empty,
}

/// An output waiting for its placeholders; its unpatched text is in the sink at its path.
#[derive(Debug)]
struct Held {
    media_type: MediaTypeId,
    format: FormatId,
    lang: LangIdx,
}

/// Publishes rendered outputs; shared by the render workers.
pub struct Publisher {
    sink: Arc<dyn Sink>,
    settings: PublishSettings,
    /// Per language: the LiveReload `<script>` element, when the language has a server.
    livereload_scripts: IdVec<LangIdx, Option<String>>,
    tokens: Mutex<UrlTokens>,
    held: Mutex<BTreeMap<OutputPath, Held>>,
    /// The `purge_css` plans whose placeholders the outputs hold.
    css_purges: Option<Arc<CssPurges>>,
    diagnostics: Arc<Diagnostics>,
    written: AtomicUsize,
}

impl std::fmt::Debug for Publisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publisher")
            .field("settings", &self.settings)
            .field("written", &self.written)
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Publisher {
    #[must_use]
    pub fn new(
        settings: PublishSettings,
        sink: Arc<dyn Sink>,
        diagnostics: Arc<Diagnostics>,
    ) -> Self {
        let mut livereload_scripts = IdVec::with_capacity(settings.sites.len());
        for s in &settings.sites {
            livereload_scripts.push(s.livereload.as_ref().map(livereload::script));
        }
        Self {
            sink,
            livereload_scripts,
            settings,
            tokens: Mutex::new(UrlTokens::new()),
            held: Mutex::new(BTreeMap::new()),
            css_purges: None,
            diagnostics,
            written: AtomicUsize::new(0),
        }
    }

    /// Resolves the placeholders of `purges` (the render session's `purge_css` plans).
    #[must_use]
    pub fn with_css_purges(mut self, purges: Arc<CssPurges>) -> Self {
        self.css_purges = Some(purges);
        self
    }

    /// `text` with its `purge_css` placeholders replaced by the CSS it uses.
    fn purge(&self, text: String) -> Result<String, PublishError> {
        let Some(purges) = &self.css_purges else {
            return Ok(text);
        };
        Ok(purges.resolve(&text, || page_names(&text))?.unwrap_or(text))
    }

    /// Publishes a rendered output.
    ///
    /// # Errors
    /// An unknown language, or a failing sink.
    pub fn emit(&self, o: Output) -> Result<Emitted, PublishError> {
        if o.text.is_empty() {
            return Ok(Emitted::Empty);
        }
        let format = self.settings.formats.get(o.format);
        let is_html = format.is_html;
        let links = self
            .settings
            .sites
            .get(o.lang)
            .ok_or(PublishError::UnknownLanguage(o.lang))?;

        let text = self.purge(o.text)?;
        let mut text = self.rewrite(text, &o.path, o.format, links);
        if is_html
            && !o.alias
            && let Some(Some(script)) = self.livereload_scripts.get(o.lang)
        {
            text = String::from_utf8(livereload::inject(text.as_bytes(), script))
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
        }
        self.add_tokens(&text, Some(&o.path));

        if has_placeholder(&text) {
            self.sink
                .write(&o.path, text.as_bytes())
                .map_err(|source| PublishError::Write {
                    path: o.path.clone(),
                    source,
                })?;
            lock(&self.held).insert(
                o.path,
                Held {
                    media_type: format.media_type,
                    format: o.format,
                    lang: o.lang,
                },
            );
            return Ok(Emitted::Held);
        }
        self.write(&o.path, &text, format.media_type)?;
        Ok(Emitted::Written)
    }

    /// The canonify / relative-URL rewrite of an output of `format` at `path` (RSS always,
    /// HTML when configured).
    fn rewrite(
        &self,
        text: String,
        path: &OutputPath,
        format: FormatId,
        links: &SiteLinks,
    ) -> String {
        let format = self.settings.formats.get(format);
        let is_html = format.is_html;
        let is_rss = format.name.eq_ignore_ascii_case("rss");
        let Some(rw) = links.rewriter(path, is_html, is_rss) else {
            return text;
        };
        let quoting = if is_html { Quoting::Html } else { Quoting::Xml };
        match rw.rewrite_str(&text, quoting) {
            std::borrow::Cow::Owned(s) => s,
            std::borrow::Cow::Borrowed(_) => text,
        }
    }

    /// Adds the URL tokens of text that is not an output of its own (`execute_as_template`
    /// results).
    pub fn add_tokens_from(&self, text: &str) {
        self.add_tokens(text, None);
    }

    fn add_tokens(&self, text: &str, from: Option<&OutputPath>) {
        let found = UrlTokens::extract(text, from);
        if !found.is_empty() {
            lock(&self.tokens).merge(found);
        }
    }

    /// Minifies (when configured) and writes.
    fn write(
        &self,
        path: &OutputPath,
        text: &str,
        media_type: MediaTypeId,
    ) -> Result<(), PublishError> {
        let minified = match &self.settings.minifier {
            Some(m) => {
                let mt = self.settings.media_types.get(media_type).type_string();
                match m.minify_media_type(&mt, text) {
                    Ok(out) => out,
                    Err(e) => {
                        self.diagnostics.push(
                            Diagnostic::warning(format!("{path}: not minified: {e}"))
                                .with_id("minify-output"),
                        );
                        text.into()
                    }
                }
            }
            None => text.into(),
        };
        self.sink
            .write(path, minified.as_bytes())
            .map_err(|source| PublishError::Write {
                path: path.clone(),
                source,
            })?;
        self.written.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// The paths of the held outputs, sorted.
    #[must_use]
    pub fn held(&self) -> Vec<OutputPath> {
        lock(&self.held).keys().cloned().collect()
    }

    /// Reads the held outputs back from the sink, replaces their placeholders (`repl`:
    /// placeholder → text), extracts their URL tokens again, then minifies and writes them. Call
    /// it outside any render.
    ///
    /// # Errors
    /// A placeholder `repl` does not cover (the output keeps its unpatched text), or a failing
    /// sink; the first error in path order is returned.
    pub fn patch_held(&self, repl: &BTreeMap<String, String>) -> Result<usize, PublishError> {
        let held = std::mem::take(&mut *lock(&self.held));
        let (keys, values): (Vec<&str>, Vec<&str>) =
            repl.iter().map(|(k, v)| (k.as_str(), v.as_str())).unzip();
        let replacer = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&keys)
            .map_err(|e| PublishError::Placeholders(e.to_string()))?;
        let results: Vec<Result<(), PublishError>> = held
            .into_par_iter()
            .map(|(path, h)| {
                let bytes = self.sink.read(&path).map_err(|source| PublishError::Read {
                    path: path.clone(),
                    source,
                })?;
                let held = String::from_utf8(bytes)
                    .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
                let text = self.purge(replacer.replace_all(&held, &values))?;
                drop(held);
                // The links of post-processed resources arrive only now: rewrite again (the
                // rewrite leaves URLs it already rewrote alone).
                let text = match self.settings.sites.get(h.lang) {
                    Some(links) => self.rewrite(text, &path, h.format, links),
                    None => text,
                };
                if let Some(placeholder) = find_placeholder(&text) {
                    return Err(PublishError::UnresolvedPlaceholder {
                        path,
                        placeholder: placeholder.to_owned(),
                    });
                }
                self.add_tokens(&text, Some(&path));
                self.write(&path, &text, h.media_type)
            })
            .collect();
        let mut written = 0;
        for r in results {
            r?;
            written += 1;
        }
        Ok(written)
    }

    /// The URL tokens found so far.
    #[must_use]
    pub fn url_tokens(&self) -> UrlTokens {
        lock(&self.tokens).clone()
    }

    /// Files written so far.
    #[must_use]
    pub fn written(&self) -> usize {
        self.written.load(Ordering::Relaxed)
    }
}

/// The names an output uses, for `purge_css`: the tags, classes and ids of its elements
/// ([`HtmlElements`]), the words of its `<script>` elements (inline scripts and
/// templates such as `type="x-tmpl-mustache"` name classes too), and every custom property it
/// mentions (`style="color: var(--x)"`).
#[must_use]
pub fn page_names(html: &str) -> PageNames {
    let e = HtmlElements::collect(html);
    let mut words: std::collections::BTreeSet<String> = purge::words(html)
        .filter(|w| w.starts_with("--") && w.len() > 2)
        .map(str::to_owned)
        .collect();
    for script in script_texts(html) {
        words.extend(purge::words(script).map(str::to_owned));
    }
    PageNames {
        tags: e.tags,
        classes: e.classes,
        ids: e.ids,
        words,
    }
}

/// The contents of the `<script>` elements of `html`.
fn script_texts(html: &str) -> Vec<&str> {
    let lower = html.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(open) = lower[at..].find("<script").map(|i| at + i) {
        let Some(start) = lower[open..].find('>').map(|i| open + i + 1) else {
            break;
        };
        let end = lower[start..]
            .find("</script")
            .map_or(html.len(), |i| start + i);
        found.push(&html[start..end]);
        at = end;
    }
    found
}

fn has_placeholder(text: &str) -> bool {
    PLACEHOLDER_PREFIXES.iter().any(|p| text.contains(p))
}

/// The first placeholder left in `text` (up to its closing `__`).
fn find_placeholder(text: &str) -> Option<&str> {
    let start = PLACEHOLDER_PREFIXES
        .iter()
        .filter_map(|p| text.find(p))
        .min()?;
    let rest = &text[start..];
    let body = rest.get(5..).unwrap_or_default();
    let end = body
        .find("__")
        .map_or(rest.len(), |e| (5 + e + 2).min(rest.len()));
    Some(&rest[..end])
}
