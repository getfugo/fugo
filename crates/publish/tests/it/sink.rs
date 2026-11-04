//! `DiskSink` and `MemorySink`: directories created on demand, and paths kept below the publish
//! directory on every system.

use std::path::Path;

use ssg_base::Sink;
use ssg_base::paths::OutputPath;
use ssg_publish::{DiskSink, MemorySink, disk_path};

#[test]
fn disk_sink_creates_directories() {
    let dir = tempfile::tempdir().unwrap();
    let sink = DiskSink::new(dir.path());
    let path = OutputPath::new("/a/b/index.html");
    assert!(!sink.exists(&path));
    sink.write(&path, b"x").unwrap();
    sink.write(&path, b"yz").unwrap();
    assert!(sink.exists(&path));
    assert_eq!(
        std::fs::read(dir.path().join("a/b/index.html")).unwrap(),
        b"yz"
    );

    let mem = MemorySink::new();
    mem.write(&path, b"m").unwrap();
    mem.write_to(&dir.path().join("mem")).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("mem/a/b/index.html")).unwrap(),
        b"m"
    );
    assert_eq!(mem.get("a/b/index.html").as_deref(), Some(&b"m"[..]));
}

#[test]
fn disk_paths_are_below_the_root() {
    let root = Path::new("public");
    let below = |p: &str| disk_path(root, &OutputPath::new(p));
    assert_eq!(below("/"), Some(root.to_owned()));
    assert_eq!(
        below("/a/b c/index.html"),
        Some(root.join("a").join("b c").join("index.html"))
    );
    // `OutputPath` resolves `..` without climbing above its root.
    assert_eq!(below("/a/../../x"), Some(root.join("x")));
}

/// Elsewhere `\` and `:` are only characters of a name.
#[cfg(not(windows))]
#[test]
fn backslashes_are_names_on_unix() {
    let root = Path::new("public");
    assert_eq!(
        disk_path(root, &OutputPath::new(r"/a/..\..\x/C:")),
        Some(root.join("a").join(r"..\..\x").join("C:"))
    );
}

/// On Windows `\` separates and `C:` is a drive: a path holding them could leave the publish
/// directory, so the sink refuses it.
#[cfg(windows)]
#[test]
fn windows_paths_cannot_leave_the_root() {
    use std::io;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("public");
    let sink = DiskSink::new(&root);
    for p in [
        r"/a/..\..\x/index.html",
        r"/\x/index.html",
        "/C:/x/index.html",
        "/C:x/index.html",
        r"/a\b/index.html",
    ] {
        let path = OutputPath::new(p);
        assert_eq!(disk_path(&root, &path), None, "{p}");
        let err = sink.write(&path, b"x").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{p}");
        assert!(!sink.exists(&path), "{p}");
    }
    assert!(!dir.path().join("x").exists());
}
