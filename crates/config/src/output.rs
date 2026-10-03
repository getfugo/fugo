//! Output formats: the built-in table and `[outputFormats]`.

use serde::{Deserialize, Serialize};
use ssg_base::{FormatId, IdVec, Map, MediaTypeId, Value};

use crate::de;
use crate::error::ConfigError;
use crate::media::MediaTypes;

/// Which template escaping a format renders with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Escaping {
    /// Contextual HTML escaping.
    Html,
    /// Plain text (`isPlainText`).
    Plain,
}

/// Whether a format's files use ugly URLs (`/a.xml` instead of `/a/index.xml`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UglyPolicy {
    /// Follow the site's `uglyURLs`.
    Inherit,
    /// Always ugly (`ugly`).
    Always,
    /// Never ugly, whatever `uglyURLs` says (`noUgly`).
    Never,
}

/// Whether pages link to this format's own URL (Go's `permalinkable`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkPolicy {
    /// `.Permalink` in this format is this format's URL.
    Own,
    /// `.Permalink` is the page's first (primary) format's URL.
    UsePrimary,
}

/// Whether the format appears in `.AlternativeOutputFormats`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Listing {
    Alternative,
    /// `notAlternative`.
    NotAlternative,
}

/// Where a format's files go in a multilingual site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    /// Under the language's directory.
    LanguageDir,
    /// At the publish root (`root`: robots.txt, sitemap index).
    Root,
}

/// An output format.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OutputFormat {
    /// Lower-case name (`html`, `rss`).
    pub name: String,
    pub media_type: MediaTypeId,
    /// File name without suffix (`index`).
    pub base_name: String,
    /// Sub-directory of the page's directory (`amp`).
    pub path: String,
    /// `rel` of the `<link>` element (`canonical`, `alternate`).
    pub rel: String,
    /// Replaces the base URL's scheme (`webcal://`).
    pub protocol: String,
    pub escaping: Escaping,
    /// An HTML format (templates get HTML-only features, such as aliases and live reload).
    pub is_html: bool,
    pub ugly: UglyPolicy,
    pub links: LinkPolicy,
    pub listing: Listing,
    pub placement: Placement,
    /// Render order: formats with a non-zero weight first (ascending), then by name.
    pub weight: i32,
}

/// Every output format of a site, in render order (weighted formats first, by ascending weight;
/// then by name).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OutputFormats {
    formats: IdVec<FormatId, OutputFormat>,
}

/// The fields of a `[outputFormats.X]` table; unset fields keep the built-in (or default)
/// value.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct FormatFields {
    media_type: Option<String>,
    base_name: Option<String>,
    path: Option<String>,
    rel: Option<String>,
    protocol: Option<String>,
    is_plain_text: Option<bool>,
    #[serde(rename = "isHTML")]
    is_html: Option<bool>,
    no_ugly: Option<bool>,
    ugly: Option<bool>,
    not_alternative: Option<bool>,
    root: Option<bool>,
    permalinkable: Option<bool>,
    weight: Option<i32>,
}

impl FormatFields {
    fn builtin(media: &str) -> Self {
        Self {
            media_type: Some(media.to_owned()),
            ..Self::default()
        }
    }
}

/// Go's built-in output formats: (name, media type, fields).
fn builtin() -> Vec<(&'static str, FormatFields)> {
    let f = FormatFields::builtin;
    vec![
        (
            "html",
            FormatFields {
                base_name: Some("index".into()),
                rel: Some("canonical".into()),
                is_html: Some(true),
                permalinkable: Some(true),
                weight: Some(10),
                ..f("text/html")
            },
        ),
        (
            "404",
            FormatFields {
                base_name: Some(String::new()),
                rel: Some(String::new()),
                is_html: Some(true),
                ugly: Some(true),
                not_alternative: Some(true),
                permalinkable: Some(true),
                ..f("text/html")
            },
        ),
        (
            "alias",
            FormatFields {
                base_name: Some(String::new()),
                rel: Some(String::new()),
                is_html: Some(true),
                ugly: Some(true),
                ..f("text/html")
            },
        ),
        (
            "amp",
            FormatFields {
                path: Some("amp".into()),
                rel: Some("amphtml".into()),
                is_html: Some(true),
                permalinkable: Some(true),
                ..f("text/html")
            },
        ),
        (
            "calendar",
            FormatFields {
                protocol: Some("webcal://".into()),
                is_plain_text: Some(true),
                ..f("text/calendar")
            },
        ),
        (
            "css",
            FormatFields {
                base_name: Some("styles".into()),
                rel: Some("stylesheet".into()),
                is_plain_text: Some(true),
                not_alternative: Some(true),
                ..f("text/css")
            },
        ),
        (
            "csv",
            FormatFields {
                is_plain_text: Some(true),
                ..f("text/csv")
            },
        ),
        (
            "gotmpl",
            FormatFields {
                base_name: Some(String::new()),
                rel: Some(String::new()),
                is_plain_text: Some(true),
                not_alternative: Some(true),
                ..f("text/x-gotmpl")
            },
        ),
        (
            "json",
            FormatFields {
                is_plain_text: Some(true),
                ..f("application/json")
            },
        ),
        (
            "markdown",
            FormatFields {
                is_plain_text: Some(true),
                ..f("text/markdown")
            },
        ),
        (
            "robots",
            FormatFields {
                base_name: Some("robots".into()),
                is_plain_text: Some(true),
                root: Some(true),
                ..f("text/plain")
            },
        ),
        (
            "rss",
            FormatFields {
                no_ugly: Some(true),
                ..f("application/rss+xml")
            },
        ),
        (
            "sitemap",
            FormatFields {
                base_name: Some("sitemap".into()),
                rel: Some("sitemap".into()),
                ugly: Some(true),
                ..f("application/xml")
            },
        ),
        (
            "sitemapindex",
            FormatFields {
                base_name: Some("sitemap".into()),
                rel: Some("sitemap".into()),
                ugly: Some(true),
                root: Some(true),
                ..f("application/xml")
            },
        ),
        (
            "webappmanifest",
            FormatFields {
                base_name: Some("manifest".into()),
                rel: Some("manifest".into()),
                is_plain_text: Some(true),
                not_alternative: Some(true),
                ..f("application/manifest+json")
            },
        ),
    ]
}

impl FormatFields {
    /// `over`'s set fields replace ours.
    fn overlay(&mut self, over: Self) {
        macro_rules! take {
            ($($f:ident),*) => { $( if over.$f.is_some() { self.$f = over.$f; } )* };
        }
        take!(
            media_type,
            base_name,
            path,
            rel,
            protocol,
            is_plain_text,
            is_html,
            no_ugly,
            ugly,
            not_alternative,
            root,
            permalinkable,
            weight
        );
    }

    fn build(self, name: &str, types: &MediaTypes) -> Result<OutputFormat, ConfigError> {
        let key = format!("outputFormats.{name}.mediaType");
        let mt = self
            .media_type
            .ok_or_else(|| ConfigError::invalid(&key, "an output format needs a media type"))?;
        let media_type = types
            .by_type(&mt)
            .ok_or_else(|| ConfigError::invalid(&key, format_args!("unknown media type {mt:?}")))?;
        let ugly = match (self.ugly.unwrap_or(false), self.no_ugly.unwrap_or(false)) {
            (true, _) => UglyPolicy::Always,
            (false, true) => UglyPolicy::Never,
            (false, false) => UglyPolicy::Inherit,
        };
        Ok(OutputFormat {
            name: name.to_owned(),
            media_type,
            base_name: self.base_name.unwrap_or_else(|| "index".to_owned()),
            path: self.path.unwrap_or_default(),
            rel: self.rel.unwrap_or_else(|| "alternate".to_owned()),
            protocol: self.protocol.unwrap_or_default(),
            escaping: if self.is_plain_text.unwrap_or(false) {
                Escaping::Plain
            } else {
                Escaping::Html
            },
            is_html: self.is_html.unwrap_or(false),
            ugly,
            links: if self.permalinkable.unwrap_or(false) {
                LinkPolicy::Own
            } else {
                LinkPolicy::UsePrimary
            },
            listing: if self.not_alternative.unwrap_or(false) {
                Listing::NotAlternative
            } else {
                Listing::Alternative
            },
            placement: if self.root.unwrap_or(false) {
                Placement::Root
            } else {
                Placement::LanguageDir
            },
            weight: self.weight.unwrap_or(0),
        })
    }
}

impl OutputFormats {
    /// The built-in formats.
    #[must_use]
    pub fn builtin(types: &MediaTypes) -> Self {
        Self::decode(&Map::new(), types).expect("the built-in formats are valid")
    }

    /// Decodes `[outputFormats]` over the built-in table: a table named like a built-in
    /// format changes only the fields it sets; a new name starts from `baseName = "index"`,
    /// `rel = "alternate"`.
    ///
    /// # Errors
    /// An entry that is not a table, or a missing or unknown media type.
    pub fn decode(config: &Map, types: &MediaTypes) -> Result<Self, ConfigError> {
        let mut fields: std::collections::BTreeMap<String, FormatFields> = builtin()
            .into_iter()
            .map(|(n, f)| (n.to_owned(), f))
            .collect();
        for (name, entry) in config.iter() {
            if name == "_merge" {
                continue;
            }
            let path = format!("outputFormats.{name}");
            let over: FormatFields = match entry {
                Value::Map(m) => de::from_map(m).map_err(|e| crate::decode_error(&path, &e))?,
                other => {
                    return Err(ConfigError::invalid(
                        &path,
                        format_args!("expected a table, found {other:?}"),
                    ));
                }
            };
            fields.entry(name.to_lowercase()).or_default().overlay(over);
        }
        let mut formats = fields
            .into_iter()
            .map(|(name, f)| f.build(&name, types))
            .collect::<Result<Vec<_>, _>>()?;
        formats.sort_by(|a, b| {
            // Weight 0 means "unweighted": after every weighted format.
            let rank = |f: &OutputFormat| (f.weight == 0, f.weight);
            rank(a).cmp(&rank(b)).then_with(|| a.name.cmp(&b.name))
        });
        Ok(Self {
            formats: formats.into(),
        })
    }

    /// The format with this id.
    #[must_use]
    pub fn get(&self, id: FormatId) -> &OutputFormat {
        &self.formats[id]
    }

    /// The id of the format named `name` (ignoring case).
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<FormatId> {
        self.formats
            .iter_enumerated()
            .find(|(_, f)| f.name.eq_ignore_ascii_case(name))
            .map(|(id, _)| id)
    }

    /// Every format with its id, in render order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (FormatId, &OutputFormat)> {
        self.formats.iter_enumerated()
    }

    /// The number of formats.
    #[must_use]
    pub fn len(&self) -> usize {
        self.formats.len()
    }

    /// Whether there are no formats.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }
}
