//! `enableGitInfo`: each content file's last commit (`SourceFile::git`) and the `:git` date,
//! from a project in a directory of a git repository in a temporary directory. Without git on
//! `PATH` the tests print `SKIPPED`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jiff::Zoned;
use ssg_site::{Model, ModelError};

use crate::support::Site;

/// git in `dir` with the committer and dates of `env`: its output, trimmed.
fn git(dir: &Path, env: &[(&str, &str)], args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .envs(env.iter().copied())
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Whether git is on `PATH`.
fn has_git() -> bool {
    let found = Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !found {
        eprintln!("SKIPPED git info tests: no git on PATH");
    }
    found
}

fn write(dir: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let p = dir.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
}

/// Commits everything in `repo` as Ann at `date` (the author date; the commit date is a day
/// later).
fn commit(repo: &Path, date: &str, committed: &str, message: &[&str]) {
    let env = [
        ("GIT_AUTHOR_NAME", "Ann"),
        ("GIT_AUTHOR_EMAIL", "ann@example.org"),
        ("GIT_AUTHOR_DATE", date),
        ("GIT_COMMITTER_NAME", "Bob"),
        ("GIT_COMMITTER_EMAIL", "bob@example.org"),
        ("GIT_COMMITTER_DATE", committed),
    ];
    git(repo, &env, &["add", "-A"]);
    let mut args = vec!["commit", "-q", "--no-verify"];
    for m in message {
        args.extend(["-m", m]);
    }
    git(repo, &env, &args);
}

const CONFIG: &str = "baseURL = \"https://example.org/\"\nenableGitInfo = true\n";

/// A repository whose directory `site/` is the project: `posts/a.md` and the Thai and quoted
/// file names were added in the first commit, `posts/b.md` changed in the second;
/// `posts/new.md` is not committed.
fn repository() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &[], &["init", "-q"]);
    let site = repo.join("site");
    write(
        &site,
        &[
            ("config.toml", CONFIG),
            (
                "content/posts/a.md",
                "---\ntitle: A\ndate: 2020-01-01\n---\n",
            ),
            (
                "content/posts/b.md",
                "---\ntitle: B\ndate: 2020-01-01\nlastmod: 2021-01-01\n---\n",
            ),
            ("content/นม/ขนม.md", "---\ntitle: Snack\n---\n"),
            ("content/posts/say \"hi\".md", "---\ntitle: Hi\n---\n"),
        ],
    );
    commit(
        &repo,
        "2024-01-02T03:04:05+07:00",
        "2024-01-03T00:00:00Z",
        &["Add posts", "Two posts.\n\nAnd a snack."],
    );
    write(
        &site,
        &[(
            "content/posts/b.md",
            "---\ntitle: B\ndate: 2020-01-01\nlastmod: 2021-01-01\n---\nMore.\n",
        )],
    );
    commit(
        &repo,
        "2025-03-04T05:06:07+07:00",
        "2025-03-05T00:00:00Z",
        &["Edit b"],
    );
    write(
        &site,
        &[(
            "content/posts/new.md",
            "---\ntitle: New\nlastmod: 2019-05-06\n---\n",
        )],
    );
    (tmp, site)
}

/// The page of the content file `rel` (`posts/a.md`).
fn page<'a>(model: &'a Model, rel: &str) -> &'a ssg_site::Page {
    model
        .pages
        .iter()
        .find(|p| p.source.as_ref().is_some_and(|s| s.file.rel == rel))
        .unwrap_or_else(|| panic!("no page of {rel}"))
}

/// A date in RFC 3339, with its offset.
fn rfc(z: Option<&Zoned>) -> String {
    z.map_or_else(String::new, |z| {
        z.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
    })
}

/// Each file has its last commit, with the subject, body, author (not the committer) and both
/// dates in their own offsets; `:git` (first in `lastmod`'s default sources) gives the author
/// date in its offset, as Go does, over front matter `lastmod`. A file git does not know has none, and its `lastmod`
/// comes from the next source. File names git would quote (non-ASCII, `"`) are found.
#[test]
fn last_commit_of_each_file() {
    if !has_git() {
        return;
    }
    let (tmp, dir) = repository();
    let model = Site::at(tmp, dir).model().unwrap();

    let a = page(&model, "posts/a.md");
    let g = a
        .source
        .as_ref()
        .unwrap()
        .git
        .as_deref()
        .expect("git info of a");
    assert_eq!(g.subject, "Add posts");
    assert_eq!(g.body, "Two posts.\n\nAnd a snack.");
    assert_eq!(
        (g.author_name.as_str(), g.author_email.as_str()),
        ("Ann", "ann@example.org")
    );
    assert_eq!(g.hash.len(), 40);
    assert!(g.hash.starts_with(&g.abbreviated_hash), "{g:?}");
    assert_eq!(rfc(Some(&g.author_date)), "2024-01-02T03:04:05+07:00");
    assert_eq!(rfc(Some(&g.commit_date)), "2024-01-03T00:00:00+00:00");
    assert_eq!(
        rfc(a.meta.dates.lastmod.as_ref()),
        "2024-01-02T03:04:05+07:00",
        "the author date, in its offset"
    );
    assert_eq!(
        rfc(a.meta.dates.date.as_ref()),
        "2020-01-01T00:00:00+00:00",
        "date is not git's"
    );

    let b = page(&model, "posts/b.md");
    let g = b.source.as_ref().unwrap().git.as_deref().unwrap();
    assert_eq!((g.subject.as_str(), g.body.as_str()), ("Edit b", ""));
    assert_eq!(
        rfc(b.meta.dates.lastmod.as_ref()),
        "2025-03-04T05:06:07+07:00",
        ":git comes before front matter lastmod"
    );

    for path in ["นม/ขนม.md", "posts/say \"hi\".md"] {
        let p = page(&model, path);
        let g = p.source.as_ref().unwrap().git.as_deref();
        assert_eq!(g.map(|g| g.subject.as_str()), Some("Add posts"), "{path}");
    }

    let new = page(&model, "posts/new.md");
    assert!(new.source.as_ref().unwrap().git.is_none(), "not committed");
    assert_eq!(
        rfc(new.meta.dates.lastmod.as_ref()),
        "2019-05-06T00:00:00+00:00"
    );
    assert!(
        !model
            .diagnostics
            .iter()
            .any(|d| d.id.as_deref() == Some("git-shallow-clone")),
        "{:?}",
        model.diagnostics
    );
}

/// Without `enableGitInfo` nothing is read: `lastmod` is front matter's.
#[test]
fn off_by_default() {
    if !has_git() {
        return;
    }
    let (tmp, dir) = repository();
    write(
        &dir,
        &[("config.toml", "baseURL = \"https://example.org/\"\n")],
    );
    let model = Site::at(tmp, dir).model().unwrap();
    let b = page(&model, "posts/b.md");
    assert!(b.source.as_ref().unwrap().git.is_none());
    assert_eq!(
        rfc(b.meta.dates.lastmod.as_ref()),
        "2021-01-01T00:00:00+00:00"
    );
}

/// A shallow clone has a warning: the files of its one commit all get that commit.
#[test]
fn shallow_clone_warns() {
    if !has_git() {
        return;
    }
    let (tmp, dir) = repository();
    let repo = dir.parent().unwrap();
    let clone = tmp.path().join("clone");
    let url = format!("file://{}", repo.display());
    git(
        tmp.path(),
        &[],
        &["clone", "-q", "--depth", "1", &url, "clone"],
    );
    let model = Site::at(tmp, clone.join("site")).model().unwrap();
    let warning = model
        .diagnostics
        .iter()
        .find(|d| d.id.as_deref() == Some("git-shallow-clone"))
        .expect("the shallow clone warning");
    assert!(
        warning.message.contains("fetch-depth: 0"),
        "{}",
        warning.message
    );
    let a = page(&model, "posts/a.md");
    let g = a.source.as_ref().unwrap().git.as_deref().unwrap();
    assert_eq!(g.subject, "Edit b", "the clone's only commit");
}

/// A project outside any repository, and `git` refused by `security.exec.allow`, are errors.
#[test]
fn errors() {
    if !has_git() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("site");
    write(
        &dir,
        &[
            ("config.toml", CONFIG),
            ("content/a.md", "---\ntitle: A\n---\n"),
        ],
    );
    let err = Site::at(tmp, dir).model().unwrap_err();
    assert!(matches!(err, ModelError::Git(_)), "{err}");
    assert!(
        err.to_string()
            .starts_with("enableGitInfo: git rev-parse: "),
        "{err}"
    );

    let (tmp, dir) = repository();
    write(
        &dir,
        &[(
            "config.toml",
            &format!("{CONFIG}[security.exec]\nallow = [\"^sass$\"]\n"),
        )],
    );
    let err = Site::at(tmp, dir).model().unwrap_err();
    assert!(err.to_string().contains("security.exec.allow"), "{err}");
}
