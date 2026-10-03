//! Media types: the built-in table, `[mediaTypes]` and `[contentTypes]`.

use std::fmt;

use serde::Serialize;
use ssg_base::{IdVec, Map, MediaTypeId, Value};

use crate::de;
use crate::error::ConfigError;

/// A media type (`application/rss+xml`) with the file suffixes it is written with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MediaType {
    /// `application`.
    pub main: String,
    /// `rss` (without the `+xml` structured-syntax suffix).
    pub sub: String,
    /// `xml` in `application/rss+xml`; empty when there is none.
    pub mime_suffix: String,
    /// File suffixes without the delimiter, lower case; the first is the one written.
    pub suffixes: Vec<String>,
    /// What goes before a suffix in file names (`.`; empty for suffix-less names).
    pub delimiter: String,
}

impl MediaType {
    /// Parses `main/sub[+suffix][;parameters]` (lower-cased; parameters dropped).
    ///
    /// # Errors
    /// When the string is not of that form.
    pub fn parse(s: &str) -> Result<Self, MediaTypeError> {
        let lower = s.to_ascii_lowercase();
        let essence = lower.split(';').next().unwrap_or_default().trim();
        let (main, rest) = essence
            .split_once('/')
            .ok_or_else(|| MediaTypeError(s.to_owned()))?;
        if main.is_empty() || rest.is_empty() || rest.contains('/') {
            return Err(MediaTypeError(s.to_owned()));
        }
        let (sub, mime_suffix) = rest.split_once('+').unwrap_or((rest, ""));
        Ok(Self {
            main: main.to_owned(),
            sub: sub.to_owned(),
            mime_suffix: mime_suffix.to_owned(),
            suffixes: Vec::new(),
            delimiter: String::new(),
        })
    }

    fn builtin(s: &str, suffixes: &[&str]) -> Self {
        let mut t = Self::parse(s).expect("built-in media types parse");
        t.suffixes = suffixes.iter().map(|&x| x.to_owned()).collect();
        t.delimiter = DEFAULT_DELIMITER.to_owned();
        t
    }

    /// `application/rss+xml`.
    #[must_use]
    pub fn type_string(&self) -> String {
        self.to_string()
    }

    /// The first suffix (`""` when there is none).
    #[must_use]
    pub fn first_suffix(&self) -> &str {
        self.suffixes.first().map_or("", String::as_str)
    }

    /// The delimiter followed by the first suffix (`.html`), or `""`.
    #[must_use]
    pub fn full_suffix(&self) -> String {
        if self.suffixes.is_empty() {
            String::new()
        } else {
            format!("{}{}", self.delimiter, self.first_suffix())
        }
    }

    /// Whether `suffix` (without delimiter) is one of this type's suffixes.
    #[must_use]
    pub fn has_suffix(&self, suffix: &str) -> bool {
        self.suffixes.iter().any(|s| s.eq_ignore_ascii_case(suffix))
    }

    /// Text formats: main type `text`, or a textual sub type (JSON, XML, SVG, TOML, YAML,
    /// JavaScript).
    #[must_use]
    pub fn is_text(&self) -> bool {
        self.main == "text"
            || matches!(
                self.sub.as_str(),
                "javascript" | "json" | "rss" | "xml" | "svg" | "toml" | "yml" | "yaml"
            )
    }

    /// `text/html`.
    #[must_use]
    pub fn is_html(&self) -> bool {
        self.main == "text" && self.sub == "html"
    }

    /// `text/markdown`.
    #[must_use]
    pub fn is_markdown(&self) -> bool {
        self.main == "text" && self.sub == "markdown"
    }
}

impl fmt::Display for MediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.main, self.sub)?;
        if !self.mime_suffix.is_empty() {
            write!(f, "+{}", self.mime_suffix)?;
        }
        Ok(())
    }
}

/// A string that is not a media type.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a media type (main/sub)")]
pub struct MediaTypeError(pub String);

/// The delimiter of every built-in media type.
pub const DEFAULT_DELIMITER: &str = ".";

/// Go's built-in media types and their suffixes.
pub const BUILTIN: &[(&str, &[&str])] = &[
    ("text/calendar", &["ics"]),
    ("text/css", &["css"]),
    ("text/x-scss", &["scss"]),
    ("text/x-sass", &["sass"]),
    ("text/csv", &["csv"]),
    ("text/html", &["html", "htm"]),
    ("text/javascript", &["js", "jsm", "mjs"]),
    ("text/typescript", &["ts"]),
    ("text/tsx", &["tsx"]),
    ("text/jsx", &["jsx"]),
    ("text/x-gotmpl", &["gotmpl"]),
    ("application/json", &["json"]),
    ("application/manifest+json", &["webmanifest"]),
    ("application/rss+xml", &["xml", "rss"]),
    ("application/xml", &["xml"]),
    ("image/svg+xml", &["svg"]),
    ("text/plain", &["txt"]),
    ("application/toml", &["toml"]),
    ("application/yaml", &["yaml", "yml"]),
    ("image/png", &["png"]),
    ("image/jpeg", &["jpg", "jpeg", "jpe", "jif", "jfif"]),
    ("image/gif", &["gif"]),
    ("image/tiff", &["tif", "tiff"]),
    ("image/bmp", &["bmp"]),
    ("image/webp", &["webp"]),
    ("font/ttf", &["ttf"]),
    ("font/otf", &["otf"]),
    ("application/pdf", &["pdf"]),
    ("text/markdown", &["md", "mdown", "markdown"]),
    ("text/asciidoc", &["adoc", "asciidoc", "ad"]),
    ("text/pandoc", &["pandoc", "pdc"]),
    ("text/rst", &["rst"]),
    ("text/org", &["org"]),
    ("video/x-msvideo", &["avi"]),
    ("video/mpeg", &["mpg", "mpeg"]),
    ("video/mp4", &["mp4"]),
    ("video/ogg", &["ogv"]),
    ("video/webm", &["webm"]),
    ("video/3gpp", &["3gpp", "3gp"]),
    ("application/wasm", &["wasm"]),
    ("application/octet-stream", &[]),
];

/// The content media types enabled by default (`[contentTypes]`).
pub const DEFAULT_CONTENT_TYPES: [&str; 6] = [
    "text/html",
    "text/markdown",
    "text/asciidoc",
    "text/pandoc",
    "text/rst",
    "text/org",
];

/// All media types of a site, sorted by their type string (so `xml` resolves to
/// `application/rss+xml` before `application/xml`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MediaTypes {
    types: IdVec<MediaTypeId, MediaType>,
}

impl Default for MediaTypes {
    fn default() -> Self {
        Self::from_types(
            BUILTIN
                .iter()
                .map(|(t, s)| MediaType::builtin(t, s))
                .collect(),
        )
    }
}

impl MediaTypes {
    fn from_types(mut v: Vec<MediaType>) -> Self {
        v.sort_by_cached_key(MediaType::type_string);
        Self { types: v.into() }
    }

    /// Decodes `[mediaTypes]` over the built-in table: an entry replaces the built-in entry of
    /// the same type; `suffixes` is a list or a string of comma- or space-separated suffixes;
    /// the delimiter defaults to `.` when there are suffixes.
    ///
    /// # Errors
    /// A key that is not a media type, or an entry that is not a table.
    pub fn decode(config: &Map) -> Result<Self, ConfigError> {
        let mut by_type: std::collections::BTreeMap<String, MediaType> = Self::default()
            .types
            .into_iter()
            .map(|t| (t.type_string(), t))
            .collect();
        for (key, entry) in config.iter() {
            if key == "_merge" {
                continue;
            }
            let path = format!("mediaTypes.{key}");
            let mut t = MediaType::parse(key).map_err(|e| ConfigError::invalid(&path, e))?;
            let entry = match entry {
                Value::Map(m) => m,
                Value::Null => {
                    by_type.insert(t.type_string(), t);
                    continue;
                }
                other => {
                    return Err(ConfigError::invalid(
                        &path,
                        format_args!("expected a table, found {other:?}"),
                    ));
                }
            };
            let fields: Entry = de::from_map(entry).map_err(|e| crate::decode_error(&path, &e))?;
            t.suffixes = fields
                .suffixes
                .iter()
                .flat_map(|s| s.split([',', ' ', '\t']))
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            t.delimiter = match fields.delimiter {
                Some(d) => d,
                None if t.suffixes.is_empty() => String::new(),
                None => DEFAULT_DELIMITER.to_owned(),
            };
            if t.delimiter.is_empty() && !t.suffixes.is_empty() {
                t.delimiter = DEFAULT_DELIMITER.to_owned();
            }
            by_type.insert(t.type_string(), t);
        }
        Ok(Self::from_types(by_type.into_values().collect()))
    }

    /// The type with this id.
    #[must_use]
    pub fn get(&self, id: MediaTypeId) -> &MediaType {
        &self.types[id]
    }

    /// The id of the type written `s` (`text/html`; case and parameters ignored).
    #[must_use]
    pub fn by_type(&self, s: &str) -> Option<MediaTypeId> {
        let want = MediaType::parse(s).ok()?.type_string();
        self.types
            .iter_enumerated()
            .find(|(_, t)| t.type_string() == want)
            .map(|(id, _)| id)
    }

    /// The first type (in type-string order) with file suffix `suffix`.
    #[must_use]
    pub fn by_suffix(&self, suffix: &str) -> Option<MediaTypeId> {
        let suffix = suffix.trim_start_matches('.');
        self.types
            .iter_enumerated()
            .find(|(_, t)| t.has_suffix(suffix))
            .map(|(id, _)| id)
    }

    /// Every type with its id, in type-string order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (MediaTypeId, &MediaType)> {
        self.types.iter_enumerated()
    }

    /// The number of types.
    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Whether there are no types.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Entry {
    suffixes: Vec<String>,
    delimiter: Option<String>,
}

/// The media types that are content (`[contentTypes]`): markup detection uses their
/// suffixes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ContentTypes(pub Vec<MediaTypeId>);

impl ContentTypes {
    /// Decodes `[contentTypes]` (keys are media types; an empty table means the defaults).
    ///
    /// # Errors
    /// A key that is not a known media type.
    pub fn decode(config: &Map, types: &MediaTypes) -> Result<Self, ConfigError> {
        let names: Vec<&str> = if config.is_empty() {
            DEFAULT_CONTENT_TYPES.to_vec()
        } else {
            config.keys().filter(|k| *k != "_merge").collect()
        };
        let mut ids = Vec::with_capacity(names.len());
        for n in names {
            let id = types.by_type(n).ok_or_else(|| {
                ConfigError::invalid(format!("contentTypes.{n}"), "unknown media type")
            })?;
            ids.push(id);
        }
        ids.sort_unstable();
        ids.dedup();
        Ok(Self(ids))
    }

    /// Whether a file with suffix `suffix` is content.
    #[must_use]
    pub fn is_content_suffix(&self, types: &MediaTypes, suffix: &str) -> bool {
        self.0.iter().any(|&id| types.get(id).has_suffix(suffix))
    }
}
