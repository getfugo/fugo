//! Output minification: HTML (minify-html), CSS (lightningcss), JS (oxc), JSON and XML/SVG.
//!
//! [`Minifier`] is built once from the `[minify]` configuration and shared (it is `Send + Sync`).
//! It is used by the publisher (`minifyOutput`, `--minify`) and by the `minify` resource
//! function. The options and their mapping from the Go implementation's `[minify.tdewolff]`
//! table are documented in [`options`].
//!
//! Output bytes do not match the Go implementation's minifier; the contract is that
//! minification never panics, is idempotent, and yields output that parses as the same type
//! (checked on the tdewolff corpora by the crate's tests).
//!
//! Invalid JavaScript, JSON or XML is an error ([`MinifyError`], with a position); a caller that
//! wants more leniency publishes the input unchanged on `Err`. CSS never fails, as in tdewolff:
//! rules lightningcss rejects (Tailwind's `@media screen(md)`, stray tokens, `@import` after
//! rules) are passed through with only comments and whitespace removed, the rest is minified.
//! HTML never fails either: minify-html leaves inline CSS/JS it cannot parse as written.

#![forbid(unsafe_code)]

mod css;
mod html;
mod js;
mod json;
pub mod options;
pub mod purge;
mod targets;
mod xml;

use std::borrow::Cow;

use ssg_config::MinifyConfig;

pub use json::JsonErrorKind;
pub use options::{DecodedOptions, IgnoredOption, Options};
pub use purge::{CssPurges, PURGE_PREFIX, PageNames, PurgeOptions, PurgePlan};
pub use ssg_config::global::MinifyTarget;
pub use targets::project_browsers;

/// A minification failure: invalid input, or an invalid option.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MinifyError {
    #[error("[minify.tdewolff] {key}: expected {expected}")]
    Option { key: String, expected: &'static str },
    #[error("JavaScript: {0}")]
    Js(String),
    #[error("JSON at byte {offset}: {kind}")]
    Json { offset: usize, kind: JsonErrorKind },
    #[error("XML at byte {offset}: {message}")]
    Xml { offset: u64, message: String },
    #[error("the HTML minifier produced invalid UTF-8")]
    HtmlEncoding,
    #[error("browserslist: {0}")]
    Browserslist(String),
    /// `purge_css`: CSS lightningcss rejects, an invalid pattern, an unknown placeholder.
    #[error("purge_css: {0}")]
    Purge(String),
}

/// The output type a media type is minified as, if any (the Go implementation's registration):
/// `text/html`, `text/css`, `(application|text)/(x-)?(java|ecma)script`,
/// `(application|text)/(x-|ld+|manifest+)?json`, `image/svg+xml`, and every other `…/xml` or
/// `…/…+xml`. Parameters (`; charset=…`) and case are ignored.
#[must_use]
pub fn target_for(media_type: &str) -> Option<MinifyTarget> {
    let essence = media_type.split(';').next().unwrap_or_default().trim();
    let essence = essence.to_ascii_lowercase();
    let (top, sub) = essence.split_once('/')?;
    let text_or_app = matches!(top, "text" | "application");
    let target = match sub {
        "html" if top == "text" => MinifyTarget::Html,
        "css" if top == "text" => MinifyTarget::Css,
        "svg+xml" if top == "image" => MinifyTarget::Svg,
        "javascript" | "x-javascript" | "ecmascript" | "x-ecmascript" if text_or_app => {
            MinifyTarget::Js
        }
        "json" | "x-json" | "ld+json" | "manifest+json" if text_or_app => MinifyTarget::Json,
        _ if sub == "xml" || sub.ends_with("+xml") => MinifyTarget::Xml,
        _ => return None,
    };
    Some(target)
}

/// The configured minifiers.
#[derive(Clone, Debug, Default)]
pub struct Minifier {
    options: Options,
    disabled: Vec<MinifyTarget>,
    ignored: Vec<IgnoredOption>,
}

impl Minifier {
    /// The minifier of a `[minify]` configuration.
    ///
    /// # Errors
    /// [`MinifyError::Option`] for an invalid `[minify.tdewolff]` value.
    pub fn new(config: &MinifyConfig) -> Result<Self, MinifyError> {
        let decoded = Options::from_tdewolff(&config.options)?;
        Ok(Self {
            ignored: decoded.ignored,
            ..Self::with_options(decoded.options, &config.disabled)
        })
    }

    /// A minifier with explicit options; `disabled` types pass through untouched.
    #[must_use]
    pub fn with_options(options: Options, disabled: &[MinifyTarget]) -> Self {
        Self {
            options,
            disabled: disabled.to_vec(),
            ignored: Vec::new(),
        }
    }

    /// The minifier with CSS browser targets ([`project_browsers`]): stand-alone CSS gets the
    /// vendor prefixes and the syntax those browsers need.
    #[must_use]
    pub fn with_browsers(mut self, browsers: Option<lightningcss::targets::Browsers>) -> Self {
        self.options.css.browsers = browsers;
        self
    }

    /// The lightningcss targets CSS is printed for (browsers, `keepCSS2`), for printers outside
    /// this crate's minifier such as `purge_css`.
    #[must_use]
    pub fn css_targets(&self) -> lightningcss::targets::Targets {
        css::targets(&self.options.css)
    }

    /// The effective options.
    #[must_use]
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// The configured `[minify.tdewolff]` keys that have no effect (for warnings).
    #[must_use]
    pub fn ignored_options(&self) -> &[IgnoredOption] {
        &self.ignored
    }

    /// Whether `target` is minified (not disabled).
    #[must_use]
    pub fn is_enabled(&self, target: MinifyTarget) -> bool {
        !self.disabled.contains(&target)
    }

    /// Minifies `input` as `target`; a disabled target is returned unchanged.
    ///
    /// # Errors
    /// Input that does not parse as `target` (HTML and CSS never fail).
    pub fn minify<'a>(
        &self,
        target: MinifyTarget,
        input: &'a str,
    ) -> Result<Cow<'a, str>, MinifyError> {
        if !self.is_enabled(target) {
            return Ok(Cow::Borrowed(input));
        }
        let o = &self.options;
        let out = match target {
            MinifyTarget::Html => html::minify(
                &o.html,
                self.is_enabled(MinifyTarget::Css),
                self.is_enabled(MinifyTarget::Js),
                input,
            )?,
            MinifyTarget::Css => css::minify(&o.css, input),
            MinifyTarget::Js => js::minify(&o.js, input)?,
            MinifyTarget::Json => json::minify(input)?,
            MinifyTarget::Svg => xml::minify(
                xml::Style {
                    comments: o.svg.comments,
                    whitespace: options::XmlWhitespace::Collapse,
                },
                input,
            )?,
            MinifyTarget::Xml => xml::minify(
                xml::Style {
                    comments: options::XmlComments::Remove,
                    whitespace: o.xml.whitespace,
                },
                input,
            )?,
        };
        Ok(Cow::Owned(out))
    }

    /// Minifies `input` by media type (see [`target_for`]); a media type without a minifier is
    /// returned unchanged.
    ///
    /// # Errors
    /// As [`Minifier::minify`].
    pub fn minify_media_type<'a>(
        &self,
        media_type: &str,
        input: &'a str,
    ) -> Result<Cow<'a, str>, MinifyError> {
        match target_for(media_type) {
            Some(target) => self.minify(target, input),
            None => Ok(Cow::Borrowed(input)),
        }
    }
}
