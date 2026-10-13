//! `enableGitInfo` end to end: a page's layout reads `page.git_info`, and the sitemap's
//! `<lastmod>` is the author date of the file's last commit. Without git on `PATH` the test
//! prints `SKIPPED`.

use std::fs;
use std::path::Path;
use std::process::Command;

use ssg_build::{BuildRequest, SinkKind, build};

use crate::support::write_files;

fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .envs([
            ("GIT_AUTHOR_NAME", "Ann"),
            ("GIT_AUTHOR_EMAIL", "ann@example.org"),
            ("GIT_AUTHOR_DATE", "2024-01-02T03:04:05+07:00"),
            ("GIT_COMMITTER_NAME", "Ann"),
            ("GIT_COMMITTER_EMAIL", "ann@example.org"),
            ("GIT_COMMITTER_DATE", "2024-01-02T03:04:05+07:00"),
        ])
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn git_info_in_layouts_and_sitemap() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("site");
    let files = [
        (
            "config.toml",
            "baseURL = \"https://example.org/\"\nenableGitInfo = true\ndisableKinds = [\"rss\", \"taxonomy\", \"term\"]\n",
        ),
        (
            "content/posts/a.md",
            "---\ntitle: A\ndate: 2020-01-01\n---\n",
        ),
        (
            "layouts/single.html",
            "{% set g = page.git_info %}{{ g.subject }}|{{ g.author_name }}|{{ g.abbreviated_hash | length }}|{{ g.author_date | date(format=\"%Y-%m-%d %H:%M %z\") }}|{{ page.lastmod.rfc3339 }}",
        ),
        (
            "layouts/list.html",
            "{% if page.git_info %}git{% else %}none{% endif %}",
        ),
    ];
    write_files(&dir, &files.map(|(p, t)| (p.to_owned(), t.to_owned())));
    if !(git(&dir, &["init", "-q"])
        && git(&dir, &["add", "-A"])
        && git(&dir, &["commit", "-q", "--no-verify", "-m", "Add a"]))
    {
        eprintln!("SKIPPED git info build: no git on PATH");
        return;
    }
    let public = tmp.path().join("public");
    let report = build(BuildRequest {
        source: dir,
        destination: Some(public.clone()),
        sink: SinkKind::Disk,
        clock: Some("2026-09-01T00:00:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    })
    .expect("build");
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);

    let read = |p: &str| fs::read_to_string(public.join(p)).expect(p);
    assert_eq!(
        read("posts/a/index.html"),
        "Add a|Ann|7|2024-01-02 03:04 +0700|2024-01-02T03:04:05+07:00"
    );
    assert_eq!(read("index.html"), "none", "the home page has no file");
    assert!(
        read("sitemap.xml").contains("<lastmod>2024-01-02T03:04:05+07:00</lastmod>"),
        "{}",
        read("sitemap.xml")
    );
}
