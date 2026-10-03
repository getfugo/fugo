//! The documentation site `docs/` (https://getfugo.github.io/), built by the binary: no error
//! and no warning — its link render hook warns about every link to a missing page, resource,
//! asset or heading — and a clean `templates check --deny-warnings`. The site needs no
//! network and no node tools. Its processed images and caches go to temporary directories, so
//! the test writes nothing into the repository.

use std::path::PathBuf;

use crate::{binary, stderr, stdout};

fn docs() -> PathBuf {
    ssg_testkit::fixture::repo_dir().join("docs")
}

#[test]
fn docs_site_builds_without_warnings() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let public = tmp.path().join("public");
    let cache = tmp.path().join("cache");
    let resources = tmp.path().join("resources");
    let o = binary(
        &docs(),
        &[
            "build",
            "-d",
            public.to_str().expect("utf-8 path"),
            "--cache-dir",
            cache.to_str().expect("utf-8 path"),
        ],
        &[("FUGO_RESOURCEDIR", resources.to_str().expect("utf-8 path"))],
    );
    let (out, err) = (stdout(&o), stderr(&o));
    assert!(o.status.success(), "the docs build failed:\n{err}\n{out}");
    let warnings: Vec<&str> = err
        .lines()
        .chain(out.lines())
        .filter(|l| l.starts_with("WARN"))
        .collect();
    assert!(
        warnings.is_empty(),
        "the docs build warned:\n{}",
        warnings.join("\n")
    );
    for f in [
        "index.html",
        "404.html",
        "search-index.json",
        "sitemap.xml",
        "getting-started/quick-start/index.html",
        "reference/functions/strings/replace/index.html",
        "reference/objects/page/index.html",
        "commands/fugo-build/index.html",
        "migrating/templates/index.html",
    ] {
        assert!(public.join(f).is_file(), "{f} was not written");
    }
}

#[test]
fn docs_templates_check_is_clean() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cache = tmp.path().join("cache");
    let o = binary(
        &docs(),
        &[
            "templates",
            "check",
            "--deny-warnings",
            "--cache-dir",
            cache.to_str().expect("utf-8 path"),
        ],
        &[],
    );
    assert!(
        o.status.success(),
        "templates check failed:\n{}\n{}",
        stdout(&o),
        stderr(&o)
    );
}
