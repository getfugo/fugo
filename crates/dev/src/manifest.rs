//! Manifests of a site build (docs/rust-port/REWRITE_PLAN.md §7.2): one extractor for the Go
//! and the Rust output alike. The schema is documented in testdata/golden/README.md; in short,
//! per file:
//!
//! - L1: the path and, when it differs, its normalised form (`norm`): `_hu_<hex>` -> `_hu_H`,
//!   fingerprints `.<16-64 hex>.` -> `.H.`;
//! - L2: HTML: `<title>`, `<link rel=canonical|alternate>`, the set of internal
//!   href/src/srcset URLs, the alias target of a redirect page; XML: the link/loc/guid texts and
//!   href attributes; JSON: the key paths and URL leaves; `_redirects`/`_headers`/`robots.txt`:
//!   the line set. URLs as [`crate::urls`] normalises them;
//! - L3: HTML: the visible text (sha256, length, words; the text itself with `--full-text`; a
//!   tag boundary is a space, except a `span`'s inside `pre`/`code`) and the heading ids;
//! - L4: size and sha256 of every file, `static` for files copied from static/; images: width,
//!   height and format; CSS/JS: non-empty and referenced by an HTML page.
//!
//! The golden manifests of the Go build also record its stats file, written next to its
//! configuration, under a `project:` key ([`PROJECT_PREFIX`]); this port writes no such file,
//! and reading a manifest leaves those entries out.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::scan::{self, collapse};
use crate::urls::SiteUrls;
use crate::{Fail, fail, json};

mod extract;

pub use extract::*;

pub const SCHEMA: &str = "ssg-manifest/1";
/// The prefix of the golden manifests' keys for files of the project directory (the Go
/// build's stats file).
pub const PROJECT_PREFIX: &str = "project:";

/// The type of a published file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Html,
    /// An HTML page that redirects (`<meta http-equiv=refresh>`).
    Alias,
    Xml,
    Json,
    Css,
    Js,
    Image,
    /// `_redirects`, `_headers`, `robots.txt`.
    Lines,
    Other,
}

impl Kind {
    /// The type of a file by its name.
    #[must_use]
    pub fn of(rel: &str) -> Kind {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if matches!(name, "_redirects" | "_headers" | "robots.txt") {
            return Kind::Lines;
        }
        let ext = Path::new(name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "html" | "htm" => Kind::Html,
            "xml" => Kind::Xml,
            "json" | "webmanifest" => Kind::Json,
            "css" => Kind::Css,
            "js" | "mjs" => Kind::Js,
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "ico" => Kind::Image,
            _ => Kind::Other,
        }
    }

    /// Its name in a manifest.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Kind::Html => "html",
            Kind::Alias => "alias",
            Kind::Xml => "xml",
            Kind::Json => "json",
            Kind::Css => "css",
            Kind::Js => "js",
            Kind::Image => "image",
            Kind::Lines => "lines",
            Kind::Other => "other",
        }
    }
}

/// The L2 facts of a file (by its type).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum L2 {
    Html {
        title: Vec<String>,
        rel: Vec<[String; 4]>,
        links: Vec<String>,
    },
    Alias {
        alias: String,
    },
    Xml {
        items: Vec<String>,
    },
    Json {
        keys: Vec<String>,
        urls: Vec<String>,
    },
    JsonError {
        error: String,
    },
    Lines {
        lines: Vec<String>,
    },
}

/// The visible text of a page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Text {
    pub sha256: String,
    /// In characters.
    pub len: usize,
    pub words: usize,
    /// The text itself (`--full-text`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct L3 {
    pub text: Text,
    pub ids: Vec<String>,
}

/// An image's width, height and format.
pub type ImageInfo = (i64, i64, String);

/// The L4 facts of an image, a stylesheet or a script.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum L4 {
    Asset {
        #[serde(rename = "nonEmpty")]
        non_empty: bool,
        /// Whether an HTML page links it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        referenced: Option<bool>,
    },
    Image {
        image: Option<ImageInfo>,
    },
}

/// The manifest entry of a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(rename = "type")]
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub norm: Option<String>,
    #[serde(rename = "L2", default, skip_serializing_if = "Option::is_none")]
    pub l2: Option<L2>,
    #[serde(rename = "L3", default, skip_serializing_if = "Option::is_none")]
    pub l3: Option<L3>,
    #[serde(rename = "L4", default, skip_serializing_if = "Option::is_none")]
    pub l4: Option<L4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(rename = "static", default, skip_serializing_if = "std::ops::Not::not")]
    pub is_static: bool,
}

/// A manifest: the entries of every published file, by path.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub site: String,
    pub pass: String,
    pub levels: Vec<String>,
    #[serde(rename = "baseURLs")]
    pub base_urls: Vec<String>,
    pub count: usize,
    #[serde(deserialize_with = "published_files")]
    pub files: BTreeMap<String, Entry>,
}

/// The entries of a manifest file without those of the project directory.
fn published_files<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Entry>, D::Error> {
    let raw = BTreeMap::<String, Value>::deserialize(d)?;
    raw.into_iter()
        .filter(|(k, _)| !k.starts_with(PROJECT_PREFIX))
        .map(|(k, v)| {
            serde_json::from_value(v)
                .map(|e| (k, e))
                .map_err(serde::de::Error::custom)
        })
        .collect()
}

/// The levels a manifest records (L1 always).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Levels {
    pub l2: bool,
    pub l3: bool,
    pub l4: bool,
}

impl Levels {
    /// The levels of a list such as `L1,L4`.
    ///
    /// # Errors
    /// An unknown level.
    pub fn parse(list: &[String]) -> Result<Levels, Fail> {
        let mut levels = Levels::default();
        for l in list {
            match l.as_str() {
                "L1" => {}
                "L2" => levels.l2 = true,
                "L3" => levels.l3 = true,
                "L4" => levels.l4 = true,
                other => return Err(fail!("unknown level {other}")),
            }
        }
        Ok(levels)
    }
}

/// The L1 normalisation of an output path: `_hu_<hex>` -> `_hu_H`, then fingerprints
/// `.<16-64 lower-case hex>` before a `.` -> `.H`.
#[must_use]
pub fn norm_path(p: &str) -> String {
    static HU: OnceLock<Regex> = OnceLock::new();
    static FINGERPRINT: OnceLock<Regex> = OnceLock::new();
    let hu = HU.get_or_init(|| Regex::new("_hu_[0-9a-f]+").expect("a valid expression"));
    let fp =
        FINGERPRINT.get_or_init(|| Regex::new(r"\.([0-9a-f]+)\.").expect("a valid expression"));
    let p = hu.replace_all(p, "_hu_H");
    // A fingerprint is followed by a dot, which may start the next one: match one at a time.
    let mut out = String::with_capacity(p.len());
    let mut at = 0;
    while let Some(m) = fp.captures_at(&p, at) {
        let whole = m.get(0).expect("a match");
        let hex = m.get(1).expect("a group").as_str();
        if (16..=64).contains(&hex.len()) {
            out.push_str(&p[at..whole.start()]);
            out.push_str(".H");
        } else {
            out.push_str(&p[at..whole.end() - 1]);
        }
        at = whole.end() - 1;
    }
    out.push_str(&p[at..]);
    out
}

impl Manifest {
    /// The manifest of an extraction.
    #[must_use]
    pub fn new(
        site: &str,
        pass: &str,
        levels: &[String],
        bases: &[String],
        files: BTreeMap<String, Entry>,
    ) -> Manifest {
        Manifest {
            schema: SCHEMA.into(),
            site: site.into(),
            pass: pass.into(),
            levels: levels.to_vec(),
            base_urls: bases.to_vec(),
            count: files.len(),
            files,
        }
    }

    /// Whether it records `level`.
    #[must_use]
    pub fn has(&self, level: &str) -> bool {
        self.levels.iter().any(|l| l == level)
    }

    /// The manifest as text: sorted keys, one file per line.
    #[must_use]
    pub fn to_text(&self) -> String {
        let head = serde_json::to_value(self).expect("a manifest serializes");
        let mut lines: Vec<String> = head
            .as_object()
            .expect("an object")
            .iter()
            .filter(|(k, _)| *k != "files")
            .map(|(k, v)| format!("{}: {}", json::string(k), json::line(v)))
            .collect();
        let body: Vec<String> = self
            .files
            .iter()
            .map(|(k, e)| format!("{}: {}", json::string(k), json::line(e)))
            .collect();
        lines.push(format!("\"files\": {{\n{}\n}}", body.join(",\n")));
        lines.sort();
        format!("{{\n{}\n}}\n", lines.join(",\n"))
    }
}

/// One line about a manifest or a structure dump.
///
/// # Errors
/// An unreadable file.
pub fn summary(path: &Path) -> Result<String, Fail> {
    let doc: Value = json::read(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let count = |k: &str| doc[k].as_array().map_or(0, Vec::len);
    if doc["schema"]
        .as_str()
        .is_some_and(|s| s.starts_with("ssg-structure/"))
    {
        let records = doc["records"].as_array().cloned().unwrap_or_default();
        let written = records
            .iter()
            .filter(|r| r["written"].as_bool().unwrap_or(true))
            .count();
        let layouts = doc["layouts"]
            .as_array()
            .map(Vec::len)
            .or_else(|| doc["layouts"].as_object().map(serde_json::Map::len));
        return Ok(format!(
            "{name}: {} (page, format) records ({written} written), {} aliases, {} page/1 aliases, {} resources, {} pages, {} layout entries",
            records.len(),
            count("aliases"),
            count("pagerAliases"),
            count("resources"),
            count("pages"),
            layouts.unwrap_or(0),
        ));
    }
    let files = doc["files"].as_object().cloned().unwrap_or_default();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for e in files.values() {
        *kinds
            .entry(e["type"].as_str().unwrap_or("").to_owned())
            .or_default() += 1;
    }
    let project = files
        .keys()
        .filter(|k| k.starts_with(PROJECT_PREFIX))
        .count();
    let levels: Vec<&str> = doc["levels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let kinds: Vec<String> = kinds.iter().map(|(k, v)| format!("{k} {v}")).collect();
    Ok(format!(
        "{name}: {} files ({} in publishDir + {project} in the project dir); levels {}; {}",
        files.len(),
        files.len() - project,
        levels.join(","),
        kinds.join(", "),
    ))
}

/// The arguments of `manifest extract`.
#[derive(clap::Args)]
pub struct Extract {
    pub publish: PathBuf,
    /// The site directory (static/ and, without --base-url, the base URLs of config.toml)
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[arg(long = "base-url")]
    pub base_urls: Vec<String>,
    #[arg(long, default_value = "L1,L2,L3,L4")]
    pub levels: String,
    #[arg(long, default_value = "")]
    pub site: String,
    #[arg(long, default_value = "")]
    pub pass: String,
    /// Record the visible text of every page
    #[arg(long)]
    pub full_text: bool,
    /// The output file (gzipped when it ends in .gz; default: stdout)
    #[arg(short, long)]
    pub out: Option<PathBuf>,
}

/// `manifest extract`.
///
/// # Errors
/// Unknown levels, or a failed extraction.
pub fn run_extract(a: &Extract) -> Result<(), Fail> {
    let level_names: Vec<String> = a
        .levels
        .split(',')
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    let levels = Levels::parse(&level_names)?;
    let bases = match (&a.base_urls[..], &a.project) {
        ([], Some(p)) => site_config(p)?,
        (given, _) => given.to_vec(),
    };
    let files = extract(
        &a.publish,
        a.project.as_deref(),
        &bases,
        levels,
        a.full_text,
    )?;
    let text = Manifest::new(&a.site, &a.pass, &level_names, &bases, files).to_text();
    match &a.out {
        Some(out) => json::write(out, &text),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}
