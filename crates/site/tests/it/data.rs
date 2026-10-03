//! `data::load` against the Go oracle `oracle/sitebuild/data/<case>.json.gz` (`.Site.Data`): the
//! tree (keys as written, directory nesting, precedence of the project over themes and of
//! subdirectories over files), the dropped values (warnings) and the rejected files (errors),
//! and the load errors. Numbers compare by value (Go decodes every JSON number as a float).
//!
//! Also the acceptance sites of T23a: the docs data files (D) and nested JSON directories.

use serde_json::Value as J;
use ssg_base::Value;
use ssg_base::diag::Severity;
use ssg_site::data::{self, Data};
use ssg_testkit::fixture::oracle;

use crate::expected;
use crate::support::{Site, all_diffs, to_json};

fn load(site: &J) -> Result<Data, ssg_site::DataError> {
    let s = Site::new(site);
    data::load(&s.vfs)
}

/// Removes the value at a `/a/b` path.
fn remove(v: &mut J, path: &str) {
    let (parent, last) = path.rsplit_once('/').unwrap();
    if let Some(J::Object(o)) = v.pointer_mut(parent) {
        o.remove(last);
    }
}

fn check(name: &str) {
    let f: J = oracle(&format!("oracle/sitebuild/data/{name}.json.gz"));
    let log: Vec<&str> = f["log"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    let result = load(&f["site"]);
    if f["data"].is_null() {
        assert!(result.is_err(), "{name}: Go fails, Rust loads {result:?}");
        eprintln!("data {name}: load error as in Go: {}", result.unwrap_err());
        return;
    }
    let d = result.unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut got = to_json(&Value::map(d.map.clone()));
    let mut want = f["data"].clone();
    let dev = expected::data(name);
    for p in &dev {
        remove(&mut got, p);
        remove(&mut want, p);
    }
    let mut diffs = Vec::new();
    all_diffs("", &got, &want, &mut diffs);
    assert!(diffs.is_empty(), "{name}: {diffs:#?}");
    let count = |s: &str| log.iter().filter(|l| l.contains(s)).count();
    let ours = |sev: Severity| d.diagnostics.iter().filter(|x| x.severity == sev).count();
    assert_eq!(
        ours(Severity::Warning),
        count("overridden by higher precedence"),
        "{name}"
    );
    // CSV is data here (a list of rows); Go rejects its [][]string.
    let rejected = count("unexpected data type") - count("[][]string");
    assert_eq!(ours(Severity::Error), rejected, "{name}");
    eprintln!(
        "data {name}: equal ({} top-level keys, {} deviations skipped)",
        d.map.len(),
        dev.len()
    );
}

#[test]
fn data_oracle_cases() {
    for name in [
        "data-basic",
        "data-theme",
        "data-themeonly",
        "data-i18n",
        "data-mounts",
        "data-ignore",
        "data-none",
        "data-badjson",
        "data-badyaml",
        "data-badtoml",
        "data-unknownext",
        "data-dirfile",
        "data-xml",
        "data-csv",
    ] {
        check(name);
    }
}

/// CSV is a list of rows (see expected_diffs.toml).
#[test]
fn data_csv_rows() {
    let f: J = oracle("oracle/sitebuild/data/data-csv.json.gz");
    let d = load(&f["site"]).unwrap();
    assert_eq!(
        to_json(&d.map["table"]),
        serde_json::json!([["a", "b"], ["1", "2"]])
    );
}

/// D: the docs site's data files (all of `docs/data`).
#[test]
fn data_docs() {
    check("data-docs");
    let f: J = oracle("oracle/sitebuild/data/data-docs.json.gz");
    let d = load(&f["site"]).unwrap();
    let files = f["site"]["files"].as_array().unwrap().len();
    assert_eq!(d.map.len(), files, "one key per docs data file");
    // Keys keep their case (`docs.chroma.lexers[0].Name`).
    let lexer = &d.map["docs"].as_map().unwrap()["chroma"].as_map().unwrap()["lexers"]
        .as_array()
        .unwrap()[0];
    assert_eq!(lexer.as_map().unwrap()["Name"], Value::string("ABAP"));
}

/// S-style nested JSON directories (`comments/2020/post-1.json`) and case preserved.
#[test]
fn data_nested_json() {
    let f: J = oracle("oracle/sitebuild/data/data-basic.json.gz");
    let d = load(&f["site"]).unwrap();
    let post = d.map["comments"].as_map().unwrap()["2020"]
        .as_map()
        .unwrap()["post-1"]
        .as_map()
        .unwrap();
    assert_eq!(post["name"], Value::string("Alice"));
    assert_eq!(
        d.map["Upper"].as_map().unwrap()["MixedCase"]
            .as_map()
            .unwrap()["Key"],
        Value::string("Value")
    );
}
