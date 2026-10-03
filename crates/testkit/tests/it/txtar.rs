//! The txtar reader: Go's format rules, the repository's archives, writing a site.

use std::fs;

use pretty_assertions::assert_eq;
use ssg_testkit::fixture::{repo_dir, testdata};
use ssg_testkit::txtar::{Archive, File};

fn file(name: &str, data: &str) -> File {
    File {
        name: name.to_owned(),
        data: data.to_owned(),
    }
}

#[test]
fn format() {
    let a = Archive::parse(
        "comment\n-- a.txt --\nA\n--  b/c.md  --\nno newline\n-- empty --\n-- x --\ntail",
    );
    assert_eq!(a.comment, "comment\n");
    assert_eq!(
        a.files,
        vec![
            file("a.txt", "A\n"),
            file("b/c.md", "no newline\n"),
            file("empty", ""),
            file("x", "tail\n")
        ]
    );
    // Not markers: no space before the closing dashes, or an indented marker.
    let b = Archive::parse("-- a --\n-- b--\n -- c --\n");
    assert_eq!(b.comment, "");
    assert_eq!(b.files, vec![file("a", "-- b--\n -- c --\n")]);
    assert_eq!(Archive::parse(""), Archive::default());
    assert_eq!(Archive::parse("only a comment").comment, "only a comment\n");
}

#[test]
fn later_file_wins() {
    let a = Archive::parse("-- f --\n1\n-- f --\n2\n");
    assert_eq!(a.get("f"), Some("2\n"));
    assert_eq!(a.get("g"), None);
}

#[test]
fn mini_site() {
    let path = testdata("oracle/commands/e2e/mini.txtar");
    let a = Archive::read(&path).unwrap();
    assert!(
        a.comment
            .starts_with("A small en/th site for the e2e oracle")
    );
    assert_eq!(a.files.len(), 30);
    assert!(
        a.get("config.toml")
            .unwrap()
            .starts_with("baseURL = \"https://example.org/\"\n")
    );
    // Round trip through a directory.
    let dir = tempfile::tempdir().unwrap();
    a.write_to(dir.path()).unwrap();
    for f in &a.files {
        let on_disk = fs::read_to_string(dir.path().join(&f.name)).unwrap();
        assert_eq!(on_disk, a.get(&f.name).unwrap(), "{}", f.name);
    }
}

#[test]
fn i01_archives_parse_like_sites_py() {
    // tools/rust-port/i01/*.txtar are read by sites.py's read_txtar; the file sets agree.
    for name in ["testsite", "errors"] {
        let path = repo_dir()
            .join("tools/rust-port/i01")
            .join(format!("{name}.txtar"));
        let text = fs::read_to_string(&path).unwrap();
        let a = Archive::parse(&text);
        let markers = text
            .lines()
            .filter(|l| l.starts_with("-- ") && l.ends_with(" --"))
            .count();
        assert_eq!(a.files.len(), markers, "{name}");
        assert!(
            a.files
                .iter()
                .all(|f| f.data.is_empty() || f.data.ends_with('\n'))
        );
    }
}

#[test]
fn rejects_names_outside_the_site() {
    let dir = tempfile::tempdir().unwrap();
    for bad in ["../x", "/abs", ""] {
        let a = Archive {
            comment: String::new(),
            files: vec![file(bad, "x\n")],
        };
        let err = a.write_to(dir.path()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "{bad:?}");
    }
}
