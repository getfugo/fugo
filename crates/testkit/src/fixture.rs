//! Plain-JSON oracle fixtures (`testdata`, this port schema).
//!
//! Fixtures are JSON documents (`.json`) or one JSON record per line (`.jsonl`), gzipped when
//! the name ends in `.gz`. Tests deserialise them into `#[derive(Deserialize)]` structs and
//! compare semantic fields only. Go values that plain JSON cannot express are single-key
//! objects whose key starts with `$nh:`; [`Tag::of`] recognises them.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// A fixture that cannot be read or decoded.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: invalid UTF-8: {source}")]
    Utf8 {
        path: PathBuf,
        source: std::string::FromUtf8Error,
    },
    #[error("{path}:{line}: {source}")]
    Json {
        path: PathBuf,
        line: usize,
        source: serde_json::Error,
    },
    #[error("{path}: not a fixture (expected .json, .jsonl, .json.gz or .jsonl.gz)")]
    Kind { path: PathBuf },
}

/// How a fixture file stores its records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// One JSON document.
    Document,
    /// One JSON record per line.
    Lines,
}

impl Layout {
    /// The layout of a fixture file, from its name.
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        let name = name.strip_suffix(".gz").unwrap_or(name);
        if name.ends_with(".jsonl") {
            Some(Self::Lines)
        } else if name.ends_with(".json") {
            Some(Self::Document)
        } else {
            None
        }
    }
}

/// The root of the checkout being tested: the repository root, which is the Cargo workspace root.
///
/// Resolved at run time: every worktree shares one cargo target dir and reuses the same
/// artifacts, so a path baked in with `env!` could point into another (possibly removed)
/// checkout. Cargo sets `CARGO_MANIFEST_DIR` for the test binary of the crate under test, and
/// every crate lives at `crates/<name>`. `FUGO_REPO_DIR` overrides both.
#[must_use]
pub fn repo_dir() -> PathBuf {
    std::env::var_os(ssg_base::env_var!("REPO_DIR"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("CARGO_MANIFEST_DIR").map(|d| PathBuf::from(d).join("../..")))
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// The `node_modules` that `tools/dev/node.sh` installs the sites' packages into (Alpine.js,
/// Turbo): `tools/dev/node_modules` of the main checkout, which every worktree
/// shares. `None` when the script does not run; the directory may not exist yet.
#[must_use]
pub fn node_tools() -> Option<PathBuf> {
    let out = std::process::Command::new(repo_dir().join("tools/dev/node.sh"))
        .arg("path")
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

/// `testdata` of the checkout being tested (see [`repo_dir`]).
#[must_use]
pub fn testdata_dir() -> PathBuf {
    repo_dir().join("testdata")
}

/// A path under `testdata`, e.g. `oracle/page/permalinks/docs.json.gz`.
#[must_use]
pub fn testdata(rel: &str) -> PathBuf {
    testdata_dir().join(rel)
}

/// The Go tree's test data that the tests read, by its path in that tree; it moved to
/// `testdata/upstream/<path>` when the Go sources were removed.
pub const UPSTREAM: [&str; 5] = [
    "testsite",
    "media/testdata/fake.png",
    "resources/images/testdata",
    "resources/testdata",
    "tpl/images/testdata",
];

/// The directory the Cargo workspace had below the repository root until it moved to the root
/// (after `44529028`); frozen fixtures still name files below it (`rust/testdata/...`).
pub const LEGACY_WORKSPACE: &str = "rust";

/// The legacy docs site (the Go implementation's documentation, as this repository's Go tree had
/// it in `docs/`): a frozen fixture of the docs gates (`tools/rust-port/i01/sites.py`) and of the
/// tests that read its content, config and assets. It moved here when `docs/` became this
/// project's own documentation; fixtures still name its files `docs/...` (see [`repo_file`]).
pub const LEGACY_DOCS: &str = "testdata/legacy-docs";

/// The directory the fixtures name the legacy docs site by (see [`LEGACY_DOCS`]); its
/// `rust-port/` subdirectory is not part of it.
pub const LEGACY_DOCS_ID: &str = "docs";

/// [`LEGACY_DOCS`] of the checkout.
#[must_use]
pub fn legacy_docs() -> PathBuf {
    repo_dir().join(LEGACY_DOCS)
}

/// The base name of the Go program's configuration file (`<name>.toml`, …) as the oracles
/// recorded it. fugo reads no file of that name as configuration.
pub const GO_CONFIG_NAME: &str = "hugo";

/// A file of the Go build's recorded output as this program writes it: the RSS `<generator>`
/// names the program that built the feed (`build.name`), so the recorded name is replaced with
/// this program's. Everything else is unchanged.
#[must_use]
pub fn go_output_as_built_here(text: &str) -> String {
    const OPEN: &str = "<generator>";
    const CLOSE: &str = "</generator>";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(OPEN) {
        let after = &rest[i + OPEN.len()..];
        let Some(j) = after.find(CLOSE) else { break };
        out.push_str(&rest[..i + OPEN.len()]);
        out.push_str(ssg_base::APP_NAME);
        rest = &after[j..];
    }
    out.push_str(rest);
    out
}

/// The path a site file the Go oracles recorded has in a local site: a configuration file
/// named [`GO_CONFIG_NAME`]`.<ext>` (the project's, a configuration directory's or a theme's)
/// is `config.<ext>`, as no such file is read. Files below a component directory (a page in
/// `content/` of that name) keep their names.
#[must_use]
pub fn local_path(rel: &str) -> String {
    const COMPONENTS: [&str; 7] = [
        "content",
        "data",
        "assets",
        "static",
        "i18n",
        "layouts",
        "archetypes",
    ];
    let (dir, name) = rel.rsplit_once('/').unwrap_or(("", rel));
    let Some(ext) = name
        .strip_prefix(GO_CONFIG_NAME)
        .and_then(|n| n.strip_prefix('.'))
    else {
        return rel.to_owned();
    };
    if !matches!(ext, "toml" | "yaml" | "yml" | "json")
        || dir.split('/').any(|seg| COMPONENTS.contains(&seg))
    {
        return rel.to_owned();
    }
    if dir.is_empty() {
        format!("config.{ext}")
    } else {
        format!("{dir}/config.{ext}")
    }
}

/// A file or directory of the checkout by its repository-relative path as the fixtures name it
/// (`repo` and `file:` ids keep the Go tree's paths): under one of [`UPSTREAM`] it is in
/// `testdata/upstream`; below [`LEGACY_DOCS_ID`] (but not `docs/rust-port`) it is in
/// [`LEGACY_DOCS`]; a path below [`LEGACY_WORKSPACE`] is that path without the prefix;
/// anything else is at `<rel>` from the repository root (see [`repo_dir`]).
#[must_use]
pub fn repo_file(rel: &str) -> PathBuf {
    let path = Path::new(rel);
    if UPSTREAM.iter().any(|p| path.starts_with(p)) {
        testdata("upstream").join(rel)
    } else if let Ok(rest) = path.strip_prefix(LEGACY_DOCS_ID)
        && !rest.starts_with("rust-port")
    {
        legacy_docs().join(rest)
    } else {
        repo_dir().join(path.strip_prefix(LEGACY_WORKSPACE).unwrap_or(path))
    }
}

fn read_text(path: &Path) -> Result<String, FixtureError> {
    let io = |source| FixtureError::Io {
        path: path.to_owned(),
        source,
    };
    let raw = fs::read(path).map_err(io)?;
    let bytes = if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::with_capacity(raw.len() * 8);
        GzDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .map_err(io)?;
        out
    } else {
        raw
    };
    String::from_utf8(bytes).map_err(|source| FixtureError::Utf8 {
        path: path.to_owned(),
        source,
    })
}

/// Reads a `.json` / `.json.gz` document.
///
/// # Errors
/// I/O, UTF-8 or JSON errors, or a file name that is not a JSON document.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, FixtureError> {
    if Layout::of(path) != Some(Layout::Document) {
        return Err(FixtureError::Kind {
            path: path.to_owned(),
        });
    }
    let text = read_text(path)?;
    serde_json::from_str(&text).map_err(|source| FixtureError::Json {
        path: path.to_owned(),
        line: source.line(),
        source,
    })
}

/// Reads the records of a `.jsonl` / `.jsonl.gz` file (blank lines are skipped).
///
/// # Errors
/// I/O, UTF-8 or JSON errors (with the 1-based line), or a file name that is not JSONL.
pub fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, FixtureError> {
    if Layout::of(path) != Some(Layout::Lines) {
        return Err(FixtureError::Kind {
            path: path.to_owned(),
        });
    }
    let text = read_text(path)?;
    text.split('\n')
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line).map_err(|source| FixtureError::Json {
                path: path.to_owned(),
                line: i + 1,
                source,
            })
        })
        .collect()
}

/// Reads a fixture under `testdata` as a document, panicking with the path on error.
///
/// # Panics
/// When the fixture cannot be read or decoded as `T`.
#[track_caller]
#[must_use]
pub fn oracle<T: DeserializeOwned>(rel: &str) -> T {
    read_json(&testdata(rel)).unwrap_or_else(|e| panic!("{e}"))
}

/// Reads the records of a JSONL fixture under `testdata`, panicking with the path on error.
///
/// # Panics
/// When the fixture cannot be read or a record cannot be decoded as `T`.
#[track_caller]
#[must_use]
pub fn oracle_lines<T: DeserializeOwned>(rel: &str) -> Vec<T> {
    read_jsonl(&testdata(rel)).unwrap_or_else(|e| panic!("{e}"))
}

/// The record count of a fixture, as `tools/dev/fixtures2json.py` defines it: the number of
/// lines of a JSONL file, the length of a top-level array, or for a top-level object the sum
/// over its members of their length (arrays and objects) or 1.
#[must_use]
pub fn records(layout: Layout, doc: &[Value]) -> usize {
    let len = |v: &Value| match v {
        Value::Array(a) => a.len(),
        Value::Object(o) => o.len(),
        _ => 1,
    };
    match (layout, doc) {
        (Layout::Lines, _) => doc.len(),
        (Layout::Document, [Value::Array(a)]) => a.len(),
        (Layout::Document, [Value::Object(o)]) => o.values().map(len).sum(),
        (Layout::Document, _) => 1,
    }
}

/// Reads any fixture file as raw JSON values (one per record for JSONL, one for a document).
///
/// # Errors
/// As [`read_json`] and [`read_jsonl`].
pub fn read_values(path: &Path) -> Result<(Layout, Vec<Value>), FixtureError> {
    match Layout::of(path) {
        Some(Layout::Lines) => Ok((Layout::Lines, read_jsonl(path)?)),
        Some(Layout::Document) => Ok((Layout::Document, vec![read_json(path)?])),
        None => Err(FixtureError::Kind {
            path: path.to_owned(),
        }),
    }
}

/// One entry of `testdata/COUNTS.json`, written when the fixtures were converted.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Counts {
    /// The fixture's path in the old port (`crates/...`, tag `go-parity-final`).
    pub from: String,
    pub records: usize,
    pub values: usize,
}

/// `testdata/COUNTS.json`: fixture path (relative to `testdata`) → counts.
///
/// # Panics
/// When the file is missing or malformed.
#[must_use]
pub fn counts() -> BTreeMap<String, Counts> {
    oracle("COUNTS.json")
}

/// A Go string: JSON text, or `{"$nh:bytes": "<hex>"}` when it is not valid UTF-8.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoString(pub Vec<u8>);

impl GoString {
    /// The string as UTF-8, when it is.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }
}

impl<'de> Deserialize<'de> for GoString {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Text(String),
            Bytes {
                #[serde(rename = "$nh:bytes")]
                hex: String,
            },
        }
        match Repr::deserialize(d)? {
            Repr::Text(s) => Ok(Self(s.into_bytes())),
            Repr::Bytes { hex } => decode_hex(&hex)
                .map(Self)
                .ok_or_else(|| serde::de::Error::custom(format!("invalid $nh:bytes {hex:?}"))),
        }
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

/// A Go value that plain JSON cannot express, as the fixture schema tags it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tag<'a> {
    /// `{"$nh:time": "2006-01-02T15:04:05.999999999+07:00[Asia/Bangkok]"}`: an RFC 9557
    /// timestamp (`Z` for UTC; the zone suffix only for IANA names).
    Time(&'a str),
    /// `{"$nh:local": "2020-01-02"}`: a TOML local date, time or date-time.
    Local(&'a str),
    /// `{"$nh:bytes": "<hex>"}`: a Go string that is not valid UTF-8.
    Bytes(&'a str),
    /// `{"$nh:float": "NaN" | "+Inf" | "-Inf"}`.
    Float(f64),
    /// `{"$nh:keys": [...]}`: only the keys of a map were recorded.
    Keys(&'a [Value]),
    /// `{"$nh:len": n}`: only the length of a slice was recorded.
    Len(u64),
}

impl<'a> Tag<'a> {
    /// The tag of `v`, or `None` for an ordinary JSON value.
    #[must_use]
    pub fn of(v: &'a Value) -> Option<Self> {
        let Value::Object(o) = v else { return None };
        if o.len() != 1 {
            return None;
        }
        let (k, v) = o.iter().next()?;
        match (k.strip_prefix("$nh:")?, v) {
            ("time", Value::String(s)) => Some(Self::Time(s)),
            ("local", Value::String(s)) => Some(Self::Local(s)),
            ("bytes", Value::String(s)) => Some(Self::Bytes(s)),
            ("float", Value::String(s)) => match s.as_str() {
                "NaN" => Some(Self::Float(f64::NAN)),
                "+Inf" => Some(Self::Float(f64::INFINITY)),
                "-Inf" => Some(Self::Float(f64::NEG_INFINITY)),
                _ => None,
            },
            ("keys", Value::Array(a)) => Some(Self::Keys(a)),
            ("len", Value::Number(n)) => n.as_u64().map(Self::Len),
            _ => None,
        }
    }
}
