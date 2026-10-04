//! The release tools: the SPDX expressions of the licence check, tools/dev/version.sh, and the
//! archives of `package`.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use ssg_dev::{licence, package, root};

#[test]
fn spdx_expressions() {
    let allowed: HashSet<String> = ["MIT", "Apache-2.0", "BSD-3-Clause", "Unicode-3.0"]
        .map(str::to_owned)
        .into();
    let cases: [(&str, Option<bool>); 21] = [
        ("MIT", Some(true)),
        ("MIT OR Apache-2.0", Some(true)),
        ("MIT AND GPL-3.0", Some(false)),
        ("GPL-3.0 OR MIT", Some(true)),
        ("(MIT OR GPL-3.0) AND Apache-2.0", Some(true)),
        ("Apache-2.0 WITH LLVM-exception", Some(true)),
        ("MIT/Apache-2.0", Some(true)),
        ("MIT / GPL-2.0", Some(true)),
        ("GPL-2.0+", Some(false)),
        ("Apache-2.0+", Some(true)),
        ("MIT AND (BSD-3-Clause OR GPL-2.0)", Some(true)),
        ("(MIT", None),
        ("MIT)", None),
        ("AND MIT", None),
        ("MIT AND", None),
        ("MIT WITH", None),
        ("()", None),
        ("MIT OR", None),
        ("(MIT OR Apache-2.0) AND Unicode-3.0", Some(true)),
        ("Zlib OR (MIT AND Apache-2.0)", Some(true)),
        ("MIT Apache-2.0", None),
    ];
    for (expr, want) in cases {
        assert_eq!(licence::evaluate(expr, &allowed).ok(), want, "{expr}");
    }
}

/// `version.sh` in a repository with the given tags: (stdout, success).
fn version(dir: &Path, args: &[&str]) -> (String, bool) {
    let out = Command::new("sh")
        .arg(root().join("tools/dev/version.sh"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("sh runs");
    (
        String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        out.status.success(),
    )
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.org",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs")
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn release_versions() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let d = tmp.path();
    git(d, &["init", "-q"]);
    git(d, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let next = |part: &str| version(d, &["next", part]);
    let ok = |v: &str| (v.to_owned(), true);
    assert_eq!(next("minor"), ok("1.0.0"), "no release yet");
    for t in ["v0.148.2", "v0.5.0"] {
        git(d, &["tag", t]);
    }
    assert_eq!(
        next("major"),
        ok("1.0.0"),
        "the Go fork's tags do not count"
    );
    for t in [
        "v1.0.0",
        "v1.9.9",
        "v1.10.0",
        "v1.11.0-rc.1",
        "v01.0.0",
        "vx",
    ] {
        git(d, &["tag", t]);
    }
    assert_eq!(next("major"), ok("2.0.0"));
    assert_eq!(next("minor"), ok("1.11.0"), "pre-releases do not count");
    assert_eq!(next("patch"), ok("1.10.1"), "versions compare as numbers");
    git(d, &["tag", "v1.11.0"]);
    assert_eq!(next("patch"), ok("1.11.1"));
    assert_eq!(version(d, &["tag", "v1.2.3-rc.1"]), ok("1.2.3-rc.1"));
    assert_eq!(version(d, &["tag", "v1.0.0"]), ok("1.0.0"));
    for bad in [
        "v0.1.0",
        "v1.2",
        "1.2.3",
        "v1.02.3",
        "v1.2.3+meta",
        "v1.2.3-",
        "",
        "v1.0.0 x",
    ] {
        assert!(!version(d, &["tag", bad]).1, "{bad:?}");
    }
    assert!(!version(d, &["next", "huge"]).1);
}

#[test]
fn go_platforms() {
    assert_eq!(
        package::go_platform("x86_64-unknown-linux-gnu").map_err(|e| e.0),
        Ok("linux-amd64".into())
    );
    assert_eq!(
        package::go_platform("aarch64-apple-darwin").map_err(|e| e.0),
        Ok("darwin-arm64".into())
    );
    assert_eq!(
        package::go_platform("x86_64-pc-windows-msvc").map_err(|e| e.0),
        Ok("windows-amd64".into())
    );
    assert!(package::go_platform("riscv64gc-unknown-linux-gnu").is_err());
    assert!(package::go_platform("x86_64-unknown-none").is_err());
}

/// `package` with a stand-in binary: the same archive twice, and its entries in the tar.gz and
/// the zip.
#[cfg(unix)]
#[test]
fn archives() {
    use std::io::Read as _;
    use std::os::unix::fs::PermissionsExt as _;

    let tmp = tempfile::tempdir().expect("a temporary directory");
    let app = package::app_name().expect("the binary's name");
    let version = std::fs::read_to_string(root().join("Cargo.toml"))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| {
            t["workspace"]["package"]["version"]
                .as_str()
                .map(str::to_owned)
        })
        .expect("the workspace version");
    let binary = tmp.path().join(&app);
    std::fs::write(
        &binary,
        format!("#!/bin/sh\necho '{app} v{version}-0123456 linux/amd64 BuildDate=unknown'\n"),
    )
    .expect("write");
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let notices = tmp.path().join("NOTICES.txt");
    std::fs::write(&notices, "notices\n").expect("write");
    let run = |target: &str, out: &Path| {
        let status = Command::new(env!("CARGO_BIN_EXE_ssg-dev"))
            .arg("package")
            .arg(&binary)
            .arg(target)
            .arg(out)
            .arg(&notices)
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .env_remove(ssg_base::env_var!("BUILD_VERSION"))
            .env_remove(ssg_base::env_var!("BUILD_COMMIT"))
            .status()
            .expect("ssg-dev runs");
        assert!(status.success(), "package {target}");
    };
    for out in ["a", "b"] {
        run("x86_64-unknown-linux-gnu", &tmp.path().join(out));
        run("x86_64-pc-windows-msvc", &tmp.path().join(out));
    }
    let name = |ext: &str, os: &str| format!("{app}_{version}_{os}-amd64.{ext}");
    for (ext, os) in [("tar.gz", "linux"), ("zip", "windows")] {
        let a = std::fs::read(tmp.path().join("a").join(name(ext, os))).expect("an archive");
        let b = std::fs::read(tmp.path().join("b").join(name(ext, os))).expect("an archive");
        assert!(a == b, "{ext}: the same input gives the same archive");
        let sha = std::fs::read_to_string(
            tmp.path()
                .join("a")
                .join(format!("{}.sha256", name(ext, os))),
        )
        .expect("a .sha256");
        assert!(sha.ends_with(&format!("  {}\n", name(ext, os))), "{sha}");
    }

    let gz = std::fs::read(tmp.path().join("a").join(name("tar.gz", "linux"))).expect("an archive");
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(&gz[..]));
    let mut names = Vec::new();
    for e in tar.entries().expect("a tar") {
        let e = e.expect("an entry");
        let h = e.header();
        assert_eq!(
            (h.uid().ok(), h.gid().ok(), h.mtime().ok()),
            (Some(0), Some(0), Some(1_700_000_000))
        );
        let name = e.path().expect("a path").to_string_lossy().into_owned();
        let mode = h.mode().expect("a mode");
        assert_eq!(
            mode,
            if name == app || name.ends_with('/') {
                0o755
            } else {
                0o644
            },
            "{name}"
        );
        names.push(name);
    }
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    for want in [
        app.as_str(),
        "LICENSE",
        "NOTICE",
        "README.md",
        "PROVENANCE.md",
        "THIRD_PARTY/",
        "THIRD_PARTY_NOTICES.txt",
    ] {
        assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
    }

    // The zip: the same entries, deflated files and directories with their Unix modes.
    let z = std::fs::read(tmp.path().join("a").join(name("zip", "windows"))).expect("an archive");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(z)).expect("a zip");
    let mut zip_names = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).expect("an entry");
        let name = f.name().to_owned();
        let mode = f.unix_mode().expect("a Unix mode");
        assert_eq!(
            mode & 0o170_000,
            if f.is_dir() { 0o040_000 } else { 0o100_000 },
            "{name}"
        );
        assert_eq!(
            mode & 0o777,
            if name == app || f.is_dir() {
                0o755
            } else {
                0o644
            },
            "{name}"
        );
        let mut data = Vec::new();
        f.read_to_end(&mut data).expect("its data");
        assert_eq!(data.len() as u64, f.size(), "{name}");
        zip_names.push(name);
    }
    assert_eq!(zip_names, names);
}
