//! Shared helpers of the oracle tests: writing a recorded site, loading its model, Go values.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jiff::Zoned;
use jiff::tz::Offset;
use serde_json::{Value as J, json};
use ssg_base::{Clock, Date, Value};
use ssg_config::{Config, LoadOptions, load};
use ssg_site::{LoadModelOptions, Model, ModelError, load_model};
use ssg_testkit::fixture::{Tag, repo_file};
use ssg_vfs::Vfs;

/// FNV-1a 64 as 16 hex digits (the oracle's hash of repository files).
fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &c in b {
        h ^= u64::from(c);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Writes the recorded site (`config.toml` and its files; repository files must still have their
/// recorded hash) into `dir`.
pub fn write_site(site: &J, dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("config.toml"), site["toml"].as_str().unwrap()).unwrap();
    for f in site["files"].as_array().unwrap() {
        let p = f["path"].as_str().unwrap();
        let content = if let Some(r) = f["repo"].as_str() {
            let b = fs::read(repo_file(r)).unwrap_or_else(|e| panic!("{r}: {e}"));
            assert_eq!(fnv(&b), f["fnv"].as_str().unwrap(), "{r} changed");
            b
        } else {
            f["content"].as_str().unwrap().as_bytes().to_vec()
        };
        let fp = dir.join(p);
        fs::create_dir_all(fp.parent().unwrap()).unwrap();
        fs::write(fp, content).unwrap();
    }
}

/// The build clock of the oracle runs: the docs pages that expired by 2026-07-31 are gone, those
/// expiring from 2026-11-18 are not.
pub fn clock() -> Clock {
    Clock("2026-09-01T00:00:00Z".parse().unwrap())
}

/// A recorded site written to a temporary directory, its config and its model.
pub struct Site {
    pub _tmp: tempfile::TempDir,
    pub dir: PathBuf,
    pub cfg: Arc<Config>,
    pub vfs: Vfs,
}

impl Site {
    pub fn new(site: &J) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(site["name"].as_str().unwrap());
        write_site(site, &dir);
        Self::at(tmp, dir)
    }

    pub fn at(tmp: tempfile::TempDir, dir: PathBuf) -> Self {
        let root = tmp.path().to_str().unwrap().to_owned();
        let cfg = load(&LoadOptions {
            source: dir.clone(),
            env: vec![("HOME".into(), format!("{root}/home"))],
            ..LoadOptions::default()
        })
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let vfs = Vfs::new(&cfg).unwrap();
        Self {
            _tmp: tmp,
            dir,
            cfg: Arc::new(cfg),
            vfs,
        }
    }

    /// The model with the site's own content filter.
    pub fn model(&self) -> Result<Model, ModelError> {
        load_model(
            self.cfg.clone(),
            &self.vfs,
            &LoadModelOptions::from_config(&self.cfg, clock()),
        )
    }

    /// The model with drafts, future and expired content included (the capture phase's view).
    pub fn model_all(&self) -> Result<Model, ModelError> {
        let mut o = LoadModelOptions::from_config(&self.cfg, clock());
        o.content.drafts = true;
        o.content.future = true;
        o.content.expired = true;
        load_model(self.cfg.clone(), &self.vfs, &o)
    }

    /// A file below the site directory as the oracle writes it (`/SITE/content/a.md`).
    pub fn norm(&self, p: &Path) -> String {
        format!(
            "/SITE/{}",
            p.strip_prefix(&self.dir).unwrap().to_str().unwrap()
        )
    }
}

/// A date the way the oracle writes it.
pub fn render_time(z: &Zoned) -> String {
    let clock = z.strftime("%Y-%m-%dT%H:%M:%S%.f").to_string();
    match z.time_zone().iana_name() {
        Some("UTC") => format!("{clock}Z"),
        Some(name) => format!("{clock}{}[{name}]", z.strftime("%:z")),
        None if z.offset() == Offset::UTC => format!("{clock}Z"),
        None => format!("{clock}{}", z.strftime("%:z")),
    }
}

/// A date as the oracle writes it, Go's zero time for `None`.
pub fn time_json(z: Option<&Zoned>) -> J {
    json!({ "$nh:time": z.map_or_else(|| "0001-01-01T00:00:00Z".to_owned(), render_time) })
}

/// A value as the oracle writes it.
pub fn to_json(v: &Value) -> J {
    match v {
        Value::Null => J::Null,
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        Value::Float(f) => json!(f),
        Value::String(s) => json!(&**s),
        Value::Date(Date::Zoned(z)) => json!({ "$nh:time": render_time(z) }),
        Value::Date(Date::Local(dt)) => json!({ "$nh:local": dt.to_string() }),
        Value::Array(a) => J::Array(a.iter().map(to_json).collect()),
        Value::Map(m) => J::Object(m.iter().map(|(k, v)| (k.to_owned(), to_json(v))).collect()),
    }
}

/// The first difference between two oracle values (`None` if equal). Numbers compare by
/// value (Go decodes JSON numbers as floats), local dates by their date and time.
pub fn diff(path: &str, got: &J, want: &J) -> Option<String> {
    match (got, want) {
        (J::Number(a), J::Number(b)) if a.as_f64() == b.as_f64() => None,
        (J::Object(a), J::Object(b)) => {
            if let (Some(Tag::Time(x)), Some(Tag::Time(y))) = (Tag::of(got), Tag::of(want)) {
                return (norm_time(x) != norm_time(y)).then(|| format!("{path}: {got} != {want}"));
            }
            if let (Some(Tag::Local(x)), Some(Tag::Local(y))) = (Tag::of(got), Tag::of(want)) {
                return (x.trim_end_matches(":00") != y.trim_end_matches(":00")
                    && !x.starts_with(y))
                .then(|| format!("{path}: {got} != {want}"));
            }
            for k in a.keys().chain(b.keys()) {
                let d = diff(
                    &format!("{path}/{k}"),
                    a.get(k).unwrap_or(&J::Null),
                    b.get(k).unwrap_or(&J::Null),
                );
                if d.is_some() {
                    return d;
                }
                if a.contains_key(k) != b.contains_key(k) {
                    return Some(format!("{path}/{k}: present only on one side"));
                }
            }
            None
        }
        (J::Array(a), J::Array(b)) if a.len() == b.len() => a
            .iter()
            .zip(b)
            .enumerate()
            .find_map(|(i, (x, y))| diff(&format!("{path}/{i}"), x, y)),
        _ if got == want => None,
        _ => Some(format!("{path}: {got} != {want}")),
    }
}

/// An oracle time without its zone name, UTC written as an offset: Go's fixed zones and
/// locations with the same offset are the same here.
fn norm_time(t: &str) -> String {
    let t = t.split_once('[').map_or(t, |(t, _)| t);
    match t.strip_suffix('Z') {
        Some(t) => format!("{t}+00:00"),
        None => t.to_owned(),
    }
}

/// Every difference between two oracle values, as [`diff`] finds them, down to the leaves.
pub fn all_diffs(path: &str, got: &J, want: &J, out: &mut Vec<String>) {
    match (got, want) {
        (J::Object(a), J::Object(b)) if Tag::of(got).is_none() && Tag::of(want).is_none() => {
            for k in a.keys().chain(b.keys().filter(|k| !a.contains_key(*k))) {
                let p = format!("{path}/{k}");
                match (a.get(k), b.get(k)) {
                    (Some(x), Some(y)) => all_diffs(&p, x, y, out),
                    _ => out.push(format!("{p}: present only on one side")),
                }
            }
        }
        (J::Array(a), J::Array(b)) if a.len() == b.len() => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                all_diffs(&format!("{path}/{i}"), x, y, out);
            }
        }
        _ => out.extend(diff(path, got, want)),
    }
}
