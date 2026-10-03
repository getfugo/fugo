//! Shared helpers of the oracle tests: fixture access, Go values, site URL helpers, output
//! formats and the tallies the pass rates are computed from.

use std::collections::BTreeMap;
use std::sync::Arc;

use jiff::Zoned;
use jiff::tz::{Offset, TimeZone};
use serde_json::Value as J;
use ssg_base::url::{Accents, BaseUrl, LinkStyle, PathCase, SiteUrls};
use ssg_base::{Date, Map, MediaTypeId, Value};
use ssg_config::OutputFormat;
use ssg_config::output::{Escaping, LinkPolicy, Listing, Placement, UglyPolicy};
use ssg_page::{PathShape, SourcePath};
use ssg_testkit::fixture::{Tag, oracle};

/// A fixture under `testdata/oracle/page/`.
pub fn fixture(rel: &str) -> J {
    oracle(&format!("oracle/page/{rel}"))
}

/// The files of a fixture family, without the ones in `skip`.
pub fn family(dir: &str, skip: &[&str]) -> Vec<String> {
    let path = ssg_testkit::fixture::testdata(&format!("oracle/page/{dir}"));
    let mut names: Vec<String> = std::fs::read_dir(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.ends_with(".json.gz") && !skip.contains(&n.as_str()))
        .collect();
    names.sort();
    names
}

pub fn s(v: &J) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}

pub fn idx(v: &J) -> usize {
    usize::try_from(v.as_u64().unwrap_or_else(|| panic!("not an index: {v}"))).expect("index")
}

/// A time zone as the oracle names it (`UTC`, `Asia/Bangkok`).
pub fn time_zone(name: &str) -> TimeZone {
    if name == "UTC" || name.is_empty() {
        TimeZone::UTC
    } else {
        TimeZone::get(name).unwrap_or_else(|e| panic!("time zone {name}: {e}"))
    }
}

/// Parses an oracle time (`2020-02-03T00:00:00+07:00[Asia/Bangkok]`, `…Z`, `…+07:00`).
pub fn parse_time(t: &str) -> Zoned {
    if let Some((stamp, zone)) = t.strip_suffix(']').and_then(|t| t.split_once('[')) {
        let ts: jiff::Timestamp = stamp.parse().unwrap_or_else(|e| panic!("{t}: {e}"));
        return ts.to_zoned(time_zone(zone));
    }
    let ts: jiff::Timestamp = t.parse().unwrap_or_else(|e| panic!("{t}: {e}"));
    if t.ends_with('Z') {
        return ts.to_zoned(TimeZone::UTC);
    }
    let off = &t[t.len() - 6..];
    let sign = if off.starts_with('-') { -1 } else { 1 };
    let h: i32 = off[1..3].parse().expect("hours");
    let m: i32 = off[4..6].parse().expect("minutes");
    let offset = Offset::from_seconds(sign * (h * 3600 + m * 60)).expect("offset");
    ts.to_zoned(TimeZone::fixed(offset))
}

const ZERO: &str = "0001-01-01T00:00:00Z";

/// An oracle time, `None` for Go's zero time.
pub fn zoned(v: &J) -> Option<Zoned> {
    let Some(Tag::Time(t)) = Tag::of(v) else {
        panic!("not a time: {v}");
    };
    (t != ZERO).then(|| parse_time(t))
}

/// A date the way the oracle writes it.
pub fn render_time(z: Option<&Zoned>) -> String {
    let Some(z) = z else {
        return ZERO.to_owned();
    };
    let clock = z.strftime("%Y-%m-%dT%H:%M:%S%.f").to_string();
    match z.time_zone().iana_name() {
        Some("UTC") => format!("{clock}Z"),
        Some(name) => format!("{clock}{}[{name}]", z.strftime("%:z")),
        None if z.offset() == Offset::UTC => format!("{clock}Z"),
        None => format!("{clock}{}", z.strftime("%:z")),
    }
}

/// A plain JSON value (with `$nh:` tags) as a front matter value.
pub fn value(v: &J) -> Value {
    if let Some(tag) = Tag::of(v) {
        return match tag {
            Tag::Time(t) => Value::Date(Date::Zoned(parse_time(t))),
            Tag::Local(t) => {
                let dt = t
                    .parse::<jiff::civil::DateTime>()
                    .or_else(|_| t.parse::<jiff::civil::Date>().map(|d| d.at(0, 0, 0, 0)))
                    .unwrap_or_else(|e| panic!("local {t}: {e}"));
                Value::Date(Date::Local(dt))
            }
            Tag::Float(f) => Value::Float(f),
            other => panic!("unexpected tag {other:?}"),
        };
    }
    match v {
        J::Null => Value::Null,
        J::Bool(b) => Value::Bool(*b),
        J::Number(n) => n
            .as_i64()
            .map_or_else(|| Value::Float(n.as_f64().expect("number")), Value::Int),
        J::String(s) => Value::string(s),
        J::Array(a) => Value::Array(Arc::new(a.iter().map(value).collect())),
        J::Object(o) => Value::map(
            o.iter()
                .map(|(k, v)| (k.as_str(), value(v)))
                .collect::<Map>(),
        ),
    }
}

/// The URL helpers of an oracle `PathSpec`.
pub fn site_urls(ps: &J) -> SiteUrls {
    let flag = |k: &str| ps[k].as_bool().unwrap_or_else(|| panic!("{k}: {ps}"));
    let urls = SiteUrls {
        base_url: BaseUrl::parse(s(&ps["baseURL"])).expect("base URL"),
        language_prefix: s(&ps["languagePrefix"]).to_owned(),
        link_style: if flag("canonifyURLs") {
            LinkStyle::Canonify
        } else {
            LinkStyle::Relative
        },
        path_case: if flag("disablePathToLower") {
            PathCase::Preserve
        } else {
            PathCase::Lower
        },
        accents: if flag("removePathAccents") {
            Accents::Remove
        } else {
            Accents::Keep
        },
    };
    assert_eq!(urls.base_path(false), s(&ps["getBasePath"]), "{ps}");
    assert_eq!(urls.base_path(true), s(&ps["getBasePathRel"]), "{ps}");
    urls
}

/// An oracle output format, with its media type's suffix (`.html`).
pub fn output_format(f: &J) -> (OutputFormat, String) {
    let flag = |k: &str| f[k].as_bool().unwrap_or_else(|| panic!("{k}: {f}"));
    let ugly = if flag("noUgly") {
        UglyPolicy::Never
    } else if flag("ugly") {
        UglyPolicy::Always
    } else {
        UglyPolicy::Inherit
    };
    let format = OutputFormat {
        name: s(&f["name"]).to_owned(),
        media_type: <MediaTypeId as ssg_base::Idx>::from_index(0),
        base_name: s(&f["baseName"]).to_owned(),
        path: s(&f["path"]).to_owned(),
        rel: s(&f["rel"]).to_owned(),
        protocol: s(&f["protocol"]).to_owned(),
        escaping: if flag("isPlainText") {
            Escaping::Plain
        } else {
            Escaping::Html
        },
        is_html: flag("isHTML"),
        ugly,
        links: if flag("permalinkable") {
            LinkPolicy::Own
        } else {
            LinkPolicy::UsePrimary
        },
        listing: if flag("notAlternative") {
            Listing::NotAlternative
        } else {
            Listing::Alternative
        },
        placement: if flag("root") {
            Placement::Root
        } else {
            Placement::LanguageDir
        },
        weight: i32::try_from(f["weight"].as_i64().expect("weight")).expect("weight"),
    };
    (format, s(&f["mediaType"]["fullSuffix"]).to_owned())
}

/// A recorded Go path (`paths` of the fixtures) with the values Go derived from it.
pub struct OraclePath {
    pub input: String,
    pub source: SourcePath,
    /// `Base()`: `/posts/my-post`.
    pub base: String,
    pub is_branch: bool,
    /// `Unnormalized().BaseNameNoIdentifier()`.
    pub unnormalized_name: String,
}

pub fn oracle_paths(entries: &J) -> Vec<OraclePath> {
    entries
        .as_array()
        .expect("paths")
        .iter()
        .map(|e| {
            let c = &e["checks"];
            let name = s(&c["baseNameNoIdentifier"]).to_owned();
            OraclePath {
                input: s(&e["input"]).to_owned(),
                source: SourcePath {
                    dir: s(&c["containerDir"]).to_owned(),
                    name: name.clone(),
                    shape: if c["isBundle"].as_bool().expect("isBundle") {
                        PathShape::Bundle
                    } else {
                        PathShape::File
                    },
                },
                base: s(&c["base"]).to_owned(),
                is_branch: s(&c["type"]) == "TypeBranch",
                unnormalized_name: c["unBaseNameNoIdentifier"]
                    .as_str()
                    .map_or(name, str::to_owned),
            }
        })
        .collect()
}

/// Pass/fail counts of one fixture family, with the first failures for the report.
#[derive(Default)]
pub struct Tally {
    pub total: usize,
    pub failed: usize,
    /// Failures accepted as documented deviations, by class.
    pub accepted: BTreeMap<&'static str, usize>,
    pub samples: Vec<String>,
}

impl Tally {
    pub fn pass(&mut self) {
        self.total += 1;
    }

    pub fn fail(&mut self, detail: impl FnOnce() -> String) {
        self.total += 1;
        self.failed += 1;
        if self.samples.len() < 25 {
            self.samples.push(detail());
        }
    }

    /// A difference that `expected_diffs.toml` documents.
    pub fn accept(&mut self, class: &'static str) {
        self.total += 1;
        *self.accepted.entry(class).or_default() += 1;
    }

    pub fn check(&mut self, ok: bool, detail: impl FnOnce() -> String) {
        if ok {
            self.pass();
        } else {
            self.fail(detail);
        }
    }

    pub fn rate(&self) -> f64 {
        let exact = self.total - self.failed - self.accepted.values().sum::<usize>();
        #[expect(clippy::cast_precision_loss, reason = "a percentage")]
        let r = exact as f64 * 100.0 / self.total.max(1) as f64;
        r
    }

    /// Prints the tally and asserts that nothing unexplained differs.
    pub fn finish(&self, family: &str) {
        eprintln!(
            "{family}: {} checks, {} exact ({:.3}%), accepted deviations {:?}, unexplained {}",
            self.total,
            self.total - self.failed - self.accepted.values().sum::<usize>(),
            self.rate(),
            self.accepted,
            self.failed
        );
        for s in &self.samples {
            eprintln!("  {s}");
        }
        assert_eq!(self.failed, 0, "{family}: unexplained differences");
        let reviewed: BTreeMap<String, usize> = expected_diffs(family);
        let got: BTreeMap<String, usize> = self
            .accepted
            .iter()
            .map(|(k, v)| ((*k).to_owned(), *v))
            .collect();
        assert_eq!(
            got, reviewed,
            "{family}: accepted deviations differ from expected_diffs.toml"
        );
    }
}

/// The reviewed deviation counts of a family (`crates/page/expected_diffs.toml`).
fn expected_diffs(family: &str) -> BTreeMap<String, usize> {
    let path = ssg_testkit::fixture::repo_dir().join("crates/page/expected_diffs.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let doc = Value::from_toml_str(&text).expect("expected_diffs.toml");
    let Some(classes) = doc
        .as_map()
        .and_then(|m| m.get(family))
        .and_then(Value::as_map)
    else {
        return BTreeMap::new();
    };
    classes
        .iter()
        .map(|(class, entry)| {
            let count = entry
                .as_map()
                .and_then(|e| e.get("count"))
                .and_then(Value::as_i64)
                .unwrap_or_else(|| panic!("{family}.{class}: no count"));
            (class.to_owned(), usize::try_from(count).expect("count"))
        })
        .collect()
}
