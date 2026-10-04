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

/// The site path a publish-dir file is served at (`a/index.html` -> `/a/`).
#[must_use]
pub fn page_url(rel: &str) -> String {
    match rel.strip_suffix("index.html") {
        Some(dir) if dir.is_empty() || dir.ends_with('/') => format!("/{dir}"),
        _ => format!("/{rel}"),
    }
}

/// Width, height and format from an image's header.
#[must_use]
pub fn image_info(bytes: &[u8]) -> Option<ImageInfo> {
    // An ICO header: count, then the first image's width and height (0 is 256).
    if bytes.starts_with(b"\0\0\x01\0") && bytes.len() >= 8 {
        let dim = |x: u8| if x == 0 { 256 } else { i64::from(x) };
        return Some((dim(bytes[6]), dim(bytes[7]), "ico".into()));
    }
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let format = match reader.format()? {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpeg",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Bmp => "bmp",
        _ => return None,
    };
    let (w, h) = reader.into_dimensions().ok()?;
    Some((w.into(), h.into(), format.into()))
}

/// The key paths (`.a.b[]`; an empty object `{}`, an empty array `[]`) and the URL leaves
/// (`<path> <url>`) of a JSON document.
fn json_structure(
    v: &Value,
    at: &str,
    keys: &mut BTreeSet<String>,
    urls: &mut Vec<String>,
    site: &SiteUrls,
    page: &str,
) {
    static INDEX: OnceLock<Regex> = OnceLock::new();
    match v {
        Value::Object(map) => {
            let mut names: Vec<&String> = map.keys().collect();
            names.sort();
            for k in names {
                json_structure(&map[k], &format!("{at}.{k}"), keys, urls, site, page);
            }
            if map.is_empty() {
                keys.insert(format!("{at}{{}}"));
            }
        }
        Value::Array(items) => {
            for (i, x) in items.iter().enumerate() {
                json_structure(x, &format!("{at}[{i}]"), keys, urls, site, page);
            }
            if items.is_empty() {
                keys.insert(format!("{at}[]"));
            }
        }
        leaf => {
            let index = INDEX.get_or_init(|| Regex::new(r"\[\d+\]").expect("a valid expression"));
            keys.insert(index.replace_all(at, "[]").into_owned());
            if let Value::String(s) = leaf
                && (s.starts_with('/') || s.contains("://"))
            {
                urls.push(format!("{at} {}", site.any(s, page)));
            }
        }
    }
}

/// The entries of a TOML table in the document's order.
fn in_order<'a, 'i>(t: &'a toml::de::DeTable<'i>) -> Vec<(&'a str, &'a toml::de::DeValue<'i>)> {
    let mut entries: Vec<(usize, &str, &toml::de::DeValue<'i>)> = t
        .iter()
        .map(|(k, v)| (k.span().start, k.get_ref().as_ref(), v.get_ref()))
        .collect();
    entries.sort_by_key(|e| e.0);
    entries.into_iter().map(|(_, k, v)| (k, v)).collect()
}

/// The base URLs of a site directory's config.toml: `baseURL` and that of each language (key
/// names case-insensitive, in the file's order).
///
/// # Errors
/// An unreadable or invalid config.toml.
pub fn site_config(project: &Path) -> Result<Vec<String>, Fail> {
    let path = project.join("config.toml");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| fail!("{}: {e}", path.display()))?;
    let doc = toml::de::DeTable::parse(&text).map_err(|e| fail!("{}: {e}", path.display()))?;
    let as_string = |v: &toml::de::DeValue<'_>| match v {
        toml::de::DeValue::String(s) => Ok(s.to_string()),
        other => Err(fail!(
            "{}: baseURL is not a string: {other:?}",
            path.display()
        )),
    };
    let top: BTreeMap<String, &toml::de::DeValue<'_>> = in_order(doc.get_ref())
        .into_iter()
        .map(|(k, v)| (k.to_lowercase(), v))
        .collect();
    let mut bases = Vec::new();
    if let Some(b) = top.get("baseurl") {
        bases.push(as_string(b)?);
    }
    if let Some(toml::de::DeValue::Table(languages)) = top.get("languages") {
        for (_, lang) in in_order(languages) {
            if let toml::de::DeValue::Table(lang) = lang {
                for (k, v) in in_order(lang) {
                    if k.eq_ignore_ascii_case("baseurl") {
                        bases.push(as_string(v)?);
                    }
                }
            }
        }
    }
    Ok(bases)
}

/// The files below `root`, as sorted `/`-separated relative paths (symbolic links to
/// directories are not entered).
#[must_use]
pub fn walk(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| !e.file_type().is_dir() && !(e.file_type().is_symlink() && e.path().is_dir()))
        .map(|e| {
            let rel = e.path().strip_prefix(root).expect("below the root");
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    out.sort();
    out
}

fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// The entry of one HTML page, and the internal links it has.
fn html_entry(
    entry: &mut Entry,
    text: &str,
    site: &SiteUrls,
    page: &str,
    levels: Levels,
    full_text: bool,
) -> BTreeSet<String> {
    let scan = scan::html(text);
    let links: BTreeSet<String> = scan
        .urls
        .iter()
        .filter_map(|u| site.internal(u, page))
        .collect();
    if let Some(target) = &scan.refresh {
        entry.kind = Kind::Alias;
        if levels.l2 {
            entry.l2 = Some(L2::Alias {
                alias: site.any(target, page),
            });
        }
        return links;
    }
    if levels.l2 {
        let rel = scan
            .rel_links
            .iter()
            .map(|r| {
                [
                    r.rel.to_owned(),
                    site.any(&r.href, page),
                    r.hreflang.clone(),
                    r.media_type.clone(),
                ]
            })
            .collect();
        entry.l2 = Some(L2::Html {
            title: scan.title.clone(),
            rel,
            links: links.iter().cloned().collect(),
        });
    }
    if levels.l3 {
        let visible = scan.visible_text();
        entry.l3 = Some(L3 {
            text: Text {
                sha256: sha256_hex(visible.as_bytes()),
                len: visible.chars().count(),
                words: visible.split_whitespace().count(),
                t: full_text.then_some(visible),
            },
            ids: scan.ids,
        });
    }
    links
}

/// The manifest entries of every file below `publish`, by path.
///
/// # Errors
/// An unreadable file or base URL.
pub fn extract(
    publish: &Path,
    project: Option<&Path>,
    bases: &[String],
    levels: Levels,
    full_text: bool,
) -> Result<BTreeMap<String, Entry>, Fail> {
    let site = SiteUrls::new(bases)?;
    let static_dir = project.map(|p| p.join("static"));
    let mut files = BTreeMap::new();
    let mut referenced = BTreeSet::new();
    for rel in walk(publish) {
        let path = publish.join(&rel);
        let bytes = std::fs::read(&path).map_err(|e| fail!("{}: {e}", path.display()))?;
        let kind = Kind::of(&rel);
        let norm = norm_path(&rel);
        let mut entry = Entry {
            kind,
            norm: (norm != rel).then_some(norm),
            l2: None,
            l3: None,
            l4: None,
            size: None,
            sha256: None,
            is_static: false,
        };
        let page = page_url(&rel);
        let text = || String::from_utf8_lossy(&bytes).into_owned();
        match kind {
            Kind::Html => {
                for link in html_entry(&mut entry, &text(), &site, &page, levels, full_text) {
                    let path = link.split(['#', '?']).next().unwrap_or("").to_owned();
                    referenced.insert(path);
                }
            }
            Kind::Xml if levels.l2 => {
                let items = scan::xml(&text())
                    .into_iter()
                    .map(|(k, v)| format!("{k} {}", site.any(&v, &page)))
                    .collect();
                entry.l2 = Some(L2::Xml { items });
            }
            Kind::Json if levels.l2 => {
                entry.l2 = Some(match serde_json::from_slice::<Value>(&bytes) {
                    Err(e) => L2::JsonError {
                        error: format!("not JSON: {e}"),
                    },
                    Ok(v) => {
                        let (mut keys, mut urls) = (BTreeSet::new(), Vec::new());
                        json_structure(&v, "", &mut keys, &mut urls, &site, &page);
                        L2::Json {
                            keys: keys.into_iter().collect(),
                            urls,
                        }
                    }
                });
            }
            Kind::Lines if levels.l2 => {
                let lines: BTreeSet<String> = text()
                    .lines()
                    .map(collapse)
                    .filter(|l| !l.is_empty())
                    .collect();
                entry.l2 = Some(L2::Lines {
                    lines: lines.into_iter().collect(),
                });
            }
            _ => {}
        }
        if levels.l4 {
            entry.size = Some(bytes.len() as u64);
            entry.sha256 = Some(sha256_hex(&bytes));
            entry.is_static = static_dir.as_ref().is_some_and(|s| s.join(&rel).is_file());
            entry.l4 = match kind {
                Kind::Image => Some(L4::Image {
                    image: image_info(&bytes),
                }),
                Kind::Css | Kind::Js => Some(L4::Asset {
                    non_empty: !bytes.trim_ascii().is_empty(),
                    referenced: None,
                }),
                _ => None,
            };
        }
        files.insert(rel, entry);
    }
    for (rel, entry) in &mut files {
        if let Some(L4::Asset { referenced: r, .. }) = &mut entry.l4 {
            *r = Some(referenced.contains(&format!("/{}", norm_path(rel))));
        }
    }
    Ok(files)
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
