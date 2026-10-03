//! The structure-oracle gate of T24 (REWRITE_PLAN.md §8.2): the alias plan of each target site
//! must list the alias files Go wrote.
//!
//! The dumps come from T01 (`tools/go-oracle/structure/`, frozen at 44529028); without
//! `testdata/golden/<site>/structure.json[.gz]` the test prints why it skips.
//! Building a site's plan also needs the site `Model` as a `NavModel` (T23b: URLs and
//! relations). **Schema** (of the frozen dumps, next to T30's `records`):
//!
//! ```json
//! { "aliases": [ { "from": "/old/a/index.html",  // the redirect file (OutputPath)
//!                  "lang": "en", "path": "/a",     // the target page ("" for the main-
//!                  "format": "html",               //   language redirect)
//!                  "kind": "front matter" } ] }    // or "redirect"; page/1 files excluded
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value as J, json};

use crate::aliases::{Row, build_rows};
use crate::support::s;

fn dump_path(dir: &Path) -> Option<PathBuf> {
    ["structure.json.gz", "structure.json"]
        .iter()
        .map(|f| dir.join(f))
        .find(|p| p.is_file())
}

/// The alias rows of a structure dump, sorted.
fn dump_rows(dump: &Path) -> Vec<Row> {
    let doc: J = ssg_testkit::fixture::read_json(dump).unwrap_or_else(|e| panic!("{e}"));
    let mut rows: Vec<Row> = doc["aliases"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|a| {
            let kind = if s(&a["kind"]) == "redirect" {
                "redirect"
            } else {
                "front matter"
            };
            (
                s(&a["from"]).to_owned(),
                s(&a["lang"]).to_owned(),
                s(&a["path"]).to_owned(),
                s(&a["format"]).to_owned(),
                kind,
            )
        })
        .collect();
    rows.sort();
    rows
}

/// The differences between a plan and a dump, as report lines.
fn diff(got: &[Row], want: &[Row]) -> Vec<String> {
    let mut out: Vec<String> = want
        .iter()
        .filter(|w| !got.contains(w))
        .map(|w| format!("missing {w:?}"))
        .collect();
    out.extend(
        got.iter()
            .filter(|g| !want.contains(g))
            .map(|g| format!("extra {g:?}")),
    );
    out
}

#[test]
fn structure_oracle_aliases() {
    let golden = ssg_testkit::fixture::repo_dir().join("testdata/golden");
    // The site labels: the directories (testdata/golden also holds its README.md).
    let mut sites: Vec<PathBuf> = fs::read_dir(&golden)
        .map(|rd| {
            rd.map(|e| e.expect("dir entry").path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    sites.sort();
    let mut found = 0;
    for dir in sites {
        let label = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match dump_path(&dir) {
            Some(dump) => {
                found += 1;
                eprintln!(
                    "structure oracle: {label}: {} alias files in {}; PENDING T23b: the site \
                     Model does not implement NavModel yet, so the plan is not built",
                    dump_rows(&dump).len(),
                    dump.display()
                );
            }
            None => eprintln!(
                "structure oracle: skipping {label}: no structure.json[.gz] (the golden data, \
                 frozen at 44529028, has none for this label)"
            ),
        }
    }
    eprintln!("structure oracle: {found} dump(s) found, 0 checked (PENDING T23b)");
}

/// The reader and comparison above on a dump written from a Go build's alias files.
#[test]
fn structure_alias_reader_self_test() {
    let (got, want) = build_rows("build-aliases.json.gz");
    let dump = json!({ "aliases": want.iter().map(|(from, lang, path, format, kind)| json!({
        "from": from, "lang": lang, "path": path, "format": format, "kind": kind,
    })).collect::<Vec<_>>() });
    let tmp = tempfile::tempdir().expect("tempdir");
    let p = tmp.path().join("structure.json");
    fs::write(&p, dump.to_string()).expect("write");
    let rows = dump_rows(&p);
    assert_eq!(rows, want);
    assert_eq!(diff(&got, &rows), Vec::<String>::new());
    assert_eq!(diff(&got[1..], &rows).len(), 1);
}
