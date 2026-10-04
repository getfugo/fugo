//! The harness's own checks: structdiff's self-test, the docs patches against the Tera patch
//! files, the changes files and the baselines, and the sites.

use std::path::Path;

use ssg_dev::{root, selftest, sites, structdiff};

/// Perturbations of Go's testsite output are classified exactly, and the ratchet rejects
/// unlisted differences (docs/rust-port/REWRITE_PLAN.md §7.2).
#[test]
fn structdiff_self_test() {
    assert_eq!(
        selftest::run(None, None, false).expect("the self-test runs"),
        0,
        "failed checks (output above)"
    );
}

/// patches.json is in its canonical form, and every layout patch has its Tera patch file in
/// sites/docs/patches/<variant>/ and the other way round.
#[test]
fn docs_patches() {
    assert_eq!(
        sites::check_patches().expect("patches.json"),
        Vec::<String>::new()
    );
}

#[test]
fn changes_files_and_baselines() {
    let dir = structdiff::changes_dir();
    let mut entries = 0;
    for e in std::fs::read_dir(&dir).expect("tools/dev/changes") {
        let path = e.expect("an entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned();
        let Some(task) = name.strip_suffix(".md").filter(|t| *t != "README") else {
            continue;
        };
        let (changes, errors) = structdiff::parse_changes(&path, task).expect("a changes file");
        assert_eq!(errors, Vec::<String>::new(), "{name}");
        entries += changes.len();
    }
    assert!(entries > 0);
    for e in std::fs::read_dir(root().join("testdata/baselines")).expect("testdata/baselines") {
        let path = e.expect("an entry").path();
        let baseline = structdiff::read_baseline(&path)
            .expect("a baseline")
            .expect("it exists");
        // Written as they are read: the ratchet rewrites only what changes.
        assert_eq!(
            structdiff::dump_baseline(&baseline),
            std::fs::read_to_string(&path).expect("a baseline"),
            "{}",
            path.display()
        );
    }
}

#[test]
fn site_names_and_variants() {
    let names = sites::list().expect("the sites");
    for n in [
        "docs",
        "testsite",
        "mini",
        "images",
        "errors",
        "probe",
        "docs-live",
        "t24-docs",
    ] {
        assert!(names.iter().any(|x| x == n), "{n}");
    }
    let sv = |n: &str, v: Option<&str>| sites::site_and_variant(n, v).map_err(|e| e.0);
    assert_eq!(
        sv("docs-live", None),
        Ok(("docs".into(), Some("live".into())))
    );
    assert_eq!(sv("docs", None), Ok(("docs".into(), Some("i01".into()))));
    assert_eq!(
        sv("docs", Some("reduced")),
        Ok(("docs".into(), Some("reduced".into())))
    );
    assert_eq!(sv("testsite", None), Ok(("testsite".into(), None)));
    assert!(sv("docs-live", Some("i01")).is_err());
    assert!(sv("testsite", Some("live")).is_err());
}

/// The testsite: a copy of testdata/upstream/testsite (times kept, as the golden builds' input
/// had them) plus the files of testsite.txtar.
#[test]
fn testsite() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let dir = tmp.path().join("testsite");
    sites::make("testsite", &dir, None, Some(&root().join("sites/testsite")))
        .expect("the testsite");
    assert!(dir.join("config.toml").is_file());
    assert!(dir.join("layouts").is_dir());
    let src = root().join("testdata/upstream/testsite/content");
    let file = std::fs::read_dir(&src)
        .expect("content")
        .map(|e| e.expect("an entry").path())
        .find(|p| p.is_file());
    if let Some(file) = file {
        let copy = dir.join("content").join(file.file_name().expect("a name"));
        let mtime = |p: &Path| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .expect("a time")
        };
        assert_eq!(mtime(&file), mtime(&copy));
    }
    assert!(
        sites::make("testsite", &dir, None, None).is_err(),
        "the directory exists"
    );
    assert!(
        sites::make("testsite", &root().join("target/x"), None, None).is_err(),
        "inside the repository"
    );
}
