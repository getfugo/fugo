//! The plain-JSON reader on three fixture families (a gzipped document per site, gzipped JSONL
//! records, an uncompressed document), the `$nh:` tags, and the T00 record counts.

use std::collections::BTreeMap;
use std::fs;

use pretty_assertions::assert_eq;
use serde::Deserialize;
use serde_json::{Value, json};
use ssg_testkit::fixture::{
    self, Counts, FixtureError, GoString, Layout, Tag, UPSTREAM, counts, legacy_docs, oracle,
    oracle_lines, read_values, records, repo_dir, repo_file, testdata,
};

fn counted(rel: &str) -> Counts {
    counts()
        .remove(rel)
        .unwrap_or_else(|| panic!("{rel} is not in COUNTS.json"))
}

fn assert_records(rel: &str) {
    let (layout, values) = read_values(&testdata(rel)).unwrap();
    assert_eq!(records(layout, &values), counted(rel).records, "{rel}");
}

// ── family 1: oracle/sitebuild/build, one gzipped JSON document per site ──

#[derive(Deserialize)]
struct BuildFixture {
    site: Site,
    renders: Vec<Value>,
}

#[derive(Deserialize)]
struct Site {
    name: String,
    toml: String,
    files: Vec<SiteFile>,
}

#[derive(Deserialize)]
struct SiteFile {
    path: String,
    content: Option<String>,
    /// A file copied from the repository instead of inline content.
    repo: Option<String>,
}

#[test]
fn sitebuild_build_sites() {
    let dir = testdata("oracle/sitebuild/build");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names.len(), 25);
    for name in &names {
        let rel = format!("oracle/sitebuild/build/{name}");
        let fx: BuildFixture = oracle(&rel);
        assert_eq!(format!("{}.json.gz", fx.site.name), *name);
        assert!(fx.site.toml.contains("baseURL"), "{rel}");
        assert!(!fx.site.files.is_empty() && !fx.renders.is_empty(), "{rel}");
        for f in &fx.site.files {
            assert!(f.content.is_some() != f.repo.is_some(), "{rel}: {}", f.path);
            if let Some(r) = &f.repo {
                assert!(repo_file(r).is_file(), "{rel}: {r}");
            }
        }
        assert_records(&rel);
    }
}

// ── family 2: oracle/transform/absurl, gzipped JSONL ──

#[derive(Debug, Deserialize)]
struct AbsUrlCase {
    #[serde(rename = "in")]
    input: GoString,
    /// The replaced text, or `panic` when Go panicked.
    out: Option<GoString>,
    panic: Option<String>,
    /// The replacer kind: `html` or `xml`.
    k: String,
    /// The base URL.
    p: String,
}

#[test]
fn transform_absurl_lines() {
    let rel = "oracle/transform/absurl/cases.jsonl.gz";
    let cases: Vec<AbsUrlCase> = oracle_lines(rel);
    assert_eq!(cases.len(), counted(rel).records);
    assert!(cases.iter().all(|c| matches!(c.k.as_str(), "html" | "xml")));
    assert!(cases.iter().all(|c| c.out.is_some() != c.panic.is_some()));
    assert_eq!(cases.iter().filter(|c| c.panic.is_some()).count(), 90);
    let first = &cases[0];
    assert_eq!(first.p, "https://example.org/");
    assert!(
        first
            .input
            .as_str()
            .unwrap()
            .contains(r#"src="/barfoo.js""#)
    );
    let out = first.out.as_ref().and_then(GoString::as_str).unwrap();
    assert!(out.contains(r#"src="https://example.org/barfoo.js""#));
    // Fuzz inputs that are not UTF-8 arrive as `$nh:bytes` and decode to the same bytes.
    let binary = cases
        .iter()
        .find(|c| c.input.as_str().is_none())
        .expect("a non-UTF-8 input");
    assert!(binary.input.0.contains(&0xff));
    assert_records("oracle/transform/absurl/inject.jsonl.gz");
}

// ── family 3: oracle/common/flect, an uncompressed document ──

#[derive(Deserialize)]
struct Flect {
    source: String,
    custom: Vec<FlectCustom>,
    /// Per input word: operation → result (a string, or `{"panic": ..}` when Go panicked).
    cases: Vec<BTreeMap<String, Value>>,
}

#[derive(Deserialize)]
struct FlectCustom {
    name: String,
    /// Absent when the custom inflections made Go fail.
    results: Option<BTreeMap<String, String>>,
}

#[test]
fn common_flect_document() {
    let rel = "oracle/common/flect/flect.json";
    let fx: Flect = oracle(rel);
    assert_eq!(fx.source, "tools/go-oracle/nh-common/flect");
    assert_eq!(fx.custom.len() + 1 + fx.cases.len(), counted(rel).records);
    assert_eq!(fx.custom[0].name, "none");
    assert_eq!(
        fx.custom[0].results.as_ref().unwrap()["capitalize:a b"],
        "A b"
    );
    let snack = fx
        .cases
        .iter()
        .find(|c| c["in"] == "snack")
        .expect("a case for `snack`");
    assert_eq!(snack["pluralize"], "snacks");
}

// ── the fixture schema ──

#[test]
fn tags() {
    let v = json!([
        {"$nh:time": "2018-02-28T00:00:00Z"},
        {"$nh:local": "2020-01-02"},
        {"$nh:bytes": "ff"},
        {"$nh:float": "-Inf"},
        {"$nh:keys": ["a"]},
        {"$nh:len": 3},
        {"$nh:time": 1},
        {"$nh:float": "1.5"},
        {"$other": "x"},
        {"t": "string", "s": "not a tag"},
    ]);
    let tags: Vec<_> = v.as_array().unwrap().iter().map(Tag::of).collect();
    assert_eq!(tags[0], Some(Tag::Time("2018-02-28T00:00:00Z")));
    assert_eq!(tags[1], Some(Tag::Local("2020-01-02")));
    assert_eq!(tags[2], Some(Tag::Bytes("ff")));
    assert_eq!(tags[3], Some(Tag::Float(f64::NEG_INFINITY)));
    assert!(matches!(tags[4], Some(Tag::Keys([Value::String(k)])) if k == "a"));
    assert_eq!(tags[5], Some(Tag::Len(3)));
    assert!(tags[6..].iter().all(Option::is_none));
}

#[test]
fn page_dates_are_tagged_times() {
    let doc: Value = oracle("oracle/sitebuild/assemble/testsite.json.gz");
    let dates = &doc["dump"]["pages"][0]["dates"];
    assert_eq!(
        Tag::of(&dates["date"]),
        Some(Tag::Time("2018-02-28T00:00:00Z"))
    );
    assert_eq!(
        Tag::of(&dates["expiryDate"]),
        Some(Tag::Time("0001-01-01T00:00:00Z"))
    );
}

#[test]
fn errors_name_the_file() {
    let err = fixture::read_json::<Value>(&testdata("oracle/commands/e2e/mini.txtar")).unwrap_err();
    assert!(matches!(err, FixtureError::Kind { .. }));
    let missing = testdata("oracle/none.json");
    let err = fixture::read_json::<Value>(&missing)
        .unwrap_err()
        .to_string();
    assert!(err.starts_with(&missing.display().to_string()), "{err}");
    assert_eq!(Layout::of(&missing), Some(Layout::Document));
}

/// The Go tree's test data is read from `testdata/upstream` by the path the fixtures record; other
/// paths, and names that only share a prefix, stay at the repository root.
#[test]
fn repo_files_of_the_go_tree() {
    let root = repo_dir();
    assert_eq!(repo_file("testsite"), testdata("upstream/testsite"));
    assert_eq!(
        repo_file("resources/testdata/exif/orientation6.jpg"),
        testdata("upstream/resources/testdata/exif/orientation6.jpg")
    );
    for rel in [
        "docsite/config.toml",
        "testsite2/config.toml",
        "resources/testdata2/a.png",
        "Cargo.toml",
        "rust-port/x",
    ] {
        assert_eq!(repo_file(rel), root.join(rel));
    }
    for p in UPSTREAM {
        assert!(repo_file(p).exists(), "{p}");
    }
}

/// Ids naming the legacy docs site as the Go tree had it (`docs/...`) are in
/// `testdata/legacy-docs`; `docs/rust-port`, this repository's own notes, stays.
#[test]
fn repo_files_of_the_legacy_docs() {
    assert_eq!(
        repo_file("docs/content/en/_index.md"),
        legacy_docs().join("content/en/_index.md")
    );
    assert!(repo_file("docs/assets/images/logos/logo-512x512.png").is_file());
    assert!(repo_file("docs/go.mod").is_file());
    assert_eq!(
        repo_file("docs/rust-port/HANDOFF.md"),
        repo_dir().join("docs/rust-port/HANDOFF.md")
    );
}

/// Ids recorded while the workspace was `rust/` (e.g. the sources of
/// `golden/images/manifest.json`) name the same files at the repository root.
#[test]
fn repo_files_of_the_legacy_workspace() {
    assert_eq!(repo_file("rust/Cargo.toml"), repo_dir().join("Cargo.toml"));
    assert_eq!(
        repo_file("rust/testdata/site-assets/repo/fuzzy-cirlcle.png"),
        testdata("site-assets/repo/fuzzy-cirlcle.png")
    );
    assert!(repo_file("rust/testdata/site-assets/repo/fuzzy-cirlcle.png").is_file());
}

/// Every converted fixture still has the record count the conversion recorded (T00 acceptance:
/// "fixtures converted, record counts match").
#[test]
fn every_fixture_matches_counts_json() {
    let all = counts();
    assert_eq!(all.len(), 236);
    for (rel, c) in &all {
        assert!(c.from.starts_with("crates/"), "{rel}");
        assert_records(rel);
    }
}
