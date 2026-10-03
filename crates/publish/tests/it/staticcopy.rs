//! The static copy against the Go oracle `commands/staticcopy` (Go's `copyStatic` with
//! spf13/fsync): file bytes, modes and modification times of the publish directory after the
//! sync, for plain, mounted, themed, symlinked and pre-existing trees and the `noTimes`,
//! `noChmod` and `cleanDestinationDir` settings.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde::Deserialize;
use ssg_config::{LoadOptions, load};
use ssg_publish::{StaticSyncOptions, sync_static_dir};
use ssg_testkit::fixture::oracle;
use ssg_vfs::{NFC_NAMES, Vfs, entry_name};

use crate::support::Tally;

#[derive(Deserialize)]
struct Fixture {
    #[serde(rename = "baseTime")]
    base_time: i64,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    entries: Vec<Entry>,
    result: Outcome,
}

#[derive(Deserialize)]
struct Entry {
    path: String,
    kind: String,
    content: Option<String>,
    hex: Option<String>,
    target: Option<String>,
    mode: Option<u32>,
    mtime: Option<i64>,
}

#[derive(Deserialize)]
struct Outcome {
    counts: BTreeMap<String, usize>,
    #[serde(rename = "publishDir")]
    publish_dir: String,
    tree: Option<Vec<Node>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Node {
    path: String,
    kind: String,
    hex: Option<String>,
    size: Option<u64>,
    mode: Option<u32>,
    /// A number, `"now"` (created by the sync), or absent (not recorded).
    mtime: Option<serde_json::Value>,
}

fn unhex(h: &str) -> Vec<u8> {
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn set_mtime(p: &Path, t: i64) {
    filetime::set_symlink_file_times(
        p,
        filetime::FileTime::from_unix_time(t, 0),
        filetime::FileTime::from_unix_time(t, 0),
    )
    .unwrap();
}

/// Writes the case's entries below `site`; every entry gets the base time unless it has its
/// own.
fn build(site: &Path, entries: &[Entry], base_time: i64) {
    fs::create_dir_all(site).unwrap();
    let mut times: Vec<(std::path::PathBuf, i64)> = Vec::new();
    // The Go program's configuration files are our `config.*`.
    for e in entries {
        let p = site.join(ssg_testkit::fixture::local_path(&e.path));
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        match e.kind.as_str() {
            "file" => {
                let bytes = match (&e.content, &e.hex) {
                    (Some(c), _) => c.clone().into_bytes(),
                    (None, Some(h)) => unhex(h),
                    (None, None) => Vec::new(),
                };
                fs::write(&p, bytes).unwrap();
                fs::set_permissions(&p, fs::Permissions::from_mode(e.mode.unwrap_or(0o644)))
                    .unwrap();
            }
            "dir" => fs::create_dir_all(&p).unwrap(),
            "symlink" => {
                std::os::unix::fs::symlink(e.target.as_deref().unwrap(), &p).unwrap();
            }
            other => panic!("entry kind {other}"),
        }
        if e.kind != "symlink" {
            times.push((p, e.mtime.unwrap_or(base_time)));
        }
    }
    // Directories (deepest first) after their content.
    let mut dirs: BTreeSet<std::path::PathBuf> = BTreeSet::new();
    for e in entries {
        let mut p = site.join(ssg_testkit::fixture::local_path(&e.path));
        while let Some(parent) = p.parent() {
            if parent == site {
                break;
            }
            dirs.insert(parent.to_path_buf());
            p = parent.to_path_buf();
        }
    }
    for (p, t) in times {
        set_mtime(&p, t);
    }
    for d in dirs.iter().rev() {
        if !fs::symlink_metadata(d).unwrap().file_type().is_symlink() {
            set_mtime(d, base_time);
        }
    }
}

/// The publish directory as the oracle records it.
fn tree(root: &Path, with_mode: bool, with_mtime: bool) -> Vec<Node> {
    let mut out = Vec::new();
    if root.exists() {
        walk(root, root, &mut out, with_mode, with_mtime);
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Node>, with_mode: bool, with_mtime: bool) {
    for e in fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        let meta = fs::metadata(&p).unwrap();
        let rel = p.strip_prefix(root).unwrap().to_str().unwrap().to_owned();
        let mtime = with_mtime.then(|| {
            serde_json::Value::from(
                filetime::FileTime::from_last_modification_time(&meta).unix_seconds(),
            )
        });
        if meta.is_dir() {
            out.push(Node {
                path: rel,
                kind: "dir".into(),
                hex: None,
                size: None,
                mode: None,
                mtime,
            });
            walk(root, &p, out, with_mode, with_mtime);
        } else {
            let bytes = fs::read(&p).unwrap();
            out.push(Node {
                path: rel,
                kind: "file".into(),
                hex: Some(hex(&bytes)),
                size: Some(bytes.len() as u64),
                mode: with_mode.then_some(meta.permissions().mode() & 0o777),
                mtime,
            });
        }
    }
}

/// Whether node `want` and `got` agree on what the oracle recorded.
fn same(want: &Node, got: &Node) -> bool {
    let mtime_ok = match &want.mtime {
        Some(serde_json::Value::Number(n)) => {
            got.mtime.as_ref() == Some(&serde_json::Value::Number(n.clone()))
        }
        _ => true,
    };
    want.kind == got.kind
        && want.hex == got.hex
        && want.size == got.size
        && (want.mode.is_none() || want.mode == got.mode)
        && mtime_ok
}

#[test]
fn staticcopy_oracle() {
    let fx: Fixture = oracle("oracle/commands/staticcopy/staticcopy.json.gz");
    let tmp = tempfile::tempdir().unwrap();
    let mut t = Tally::default();
    for case in &fx.cases {
        let root = tmp.path().join(&case.name);
        let site = root.join("site");
        build(&site, &case.entries, fx.base_time);
        let cfg = load(&LoadOptions {
            source: site.clone(),
            env: vec![("HOME".into(), root.join("home").to_str().unwrap().into())],
            ..LoadOptions::default()
        })
        .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        let publish = root.join(
            case.result
                .publish_dir
                .strip_prefix("$ROOT/")
                .expect("publishDir under $ROOT"),
        );
        assert_eq!(site.join(&cfg.dirs.publish), publish, "{}", case.name);
        let options = StaticSyncOptions::from_config(&cfg);
        let vfs = Vfs::new(&cfg).unwrap();
        let count = sync_static_dir(&vfs, &publish, &options)
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));

        let want_tree = case.result.tree.clone().unwrap_or_default();
        // The mounts that hold a file of the tree.
        let sources = |path: &str| {
            case.entries
                .iter()
                .filter(|e| !e.path.starts_with("public/") && e.path.ends_with(&format!("/{path}")))
                .count()
        };
        let overlapping = want_tree
            .iter()
            .any(|n| n.kind == "file" && sources(&n.path) > 1);

        match case.result.counts.get("") {
            Some(&want) if want == count => t.pass(),
            Some(&want) if overlapping && want > count => t.accept("shadowed-files-counted"),
            None if count == 0 => t.accept("missing-static-dir"),
            want => t.fail(|| format!("{}: count {count}, want {want:?}", case.name)),
        }

        let with_mode = want_tree.iter().any(|n| n.mode.is_some());
        let with_mtime = want_tree.iter().any(|n| n.mtime.is_some());
        let got: BTreeMap<String, Node> = tree(&publish, with_mode, with_mtime)
            .into_iter()
            .map(|n| (n.path.clone(), n))
            .collect();
        // Go recorded the oracle on Linux; on macOS the static files are published under the
        // NFC form of their names, as Go publishes them there.
        let want: BTreeMap<String, &Node> = want_tree
            .iter()
            .map(|n| (entry_name(&n.path, NFC_NAMES).into_owned(), n))
            .collect();
        for (path, w) in &want {
            match got.get(path) {
                Some(g) if same(w, g) => t.pass(),
                None if w.kind == "dir"
                    && !want.keys().any(|p| p.starts_with(&format!("{path}/"))) =>
                {
                    t.accept("empty-dirs-not-copied");
                }
                g => t.fail(|| format!("{} {path}: got {g:?}, want {w:?}", case.name)),
            }
        }
        for path in got.keys() {
            if !want.contains_key(path) {
                t.fail(|| format!("{} {path}: not in the oracle", case.name));
            }
        }
    }
    t.finish("staticcopy");
}

/// `sync_static` copies the same files into any sink, below the language directory on
/// multihost sites.
#[test]
fn static_into_memory() {
    let fx: Fixture = oracle("oracle/commands/staticcopy/staticcopy.json.gz");
    let case = fx.cases.iter().find(|c| c.name == "basic").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let site = tmp.path().join("site");
    build(&site, &case.entries, fx.base_time);
    let cfg = load(&LoadOptions {
        source: site,
        env: vec![(
            "HOME".into(),
            tmp.path().join("home").to_str().unwrap().into(),
        )],
        ..LoadOptions::default()
    })
    .unwrap();
    let vfs = Vfs::new(&cfg).unwrap();
    let sink = ssg_publish::MemorySink::new();
    let mut options = StaticSyncOptions::from_config(&cfg);
    assert_eq!(ssg_publish::sync_static(&vfs, &sink, &options).unwrap(), 18);
    assert_eq!(sink.text("/ไทย/หน้า.html").as_deref(), Some("<p>ไทย</p>\n"));
    assert!(sink.get("/.hidden/secret.txt").is_some());

    let mut dirs = ssg_base::IdVec::with_capacity(1);
    dirs.push("en".to_owned());
    options.language_dirs = Some(dirs);
    let sink = ssg_publish::MemorySink::new();
    ssg_publish::sync_static(&vfs, &sink, &options).unwrap();
    assert_eq!(
        sink.text("/en/robots.txt").as_deref(),
        Some("User-agent: *\n")
    );
}
