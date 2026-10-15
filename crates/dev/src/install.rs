//! `cargo dev install`: installs the program from this checkout, named as CI names a release
//! build (DEVELOPMENT.md, "CI and releases").
//!
//! `cargo install --path crates/cli --locked`, followed by the command's arguments (`--root
//! <dir>`, `--features …`), runs with `$<PREFIX>_BUILD_VERSION`, `$<PREFIX>_BUILD_COMMIT` and
//! `$<PREFIX>_BUILD_DATE` read from git ([`Build::read`]), so that `<name> version` prints
//! `<name> v<version>-<commit> <os>/<arch> BuildDate=<date>`. The version is that of the latest
//! release tag reachable from `HEAD`, so a build past a tag prints the tag's version with its own
//! commit, which tells it from the release.

use std::path::Path;
use std::process::Command;

use ssg_base::env_var;

use crate::{Fail, fail};

/// What a build of the checkout is named after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    /// The highest release tag reachable from `HEAD`, without its `v` (pre-releases count);
    /// none without one (a shallow clone), and the build has the workspace's version.
    pub version: Option<String>,
    /// `HEAD`'s commit.
    pub commit: String,
    /// The commit's time in UTC, RFC 3339 (`<yyyy>-<mm>-<dd>T<hh>:<mm>:<ss>Z`).
    pub date: String,
}

impl Build {
    /// The build of the git checkout at `repo`.
    ///
    /// # Errors
    /// `repo` is not a git checkout, or git fails.
    pub fn read(repo: &Path) -> Result<Self, Fail> {
        let commit = git(repo, &["rev-parse", "HEAD"])?;
        let seconds = git(repo, &["show", "-s", "--format=%ct", "HEAD"])?;
        let date = seconds
            .parse()
            .ok()
            .and_then(|s| jiff::Timestamp::from_second(s).ok())
            .ok_or_else(|| fail!("git: the commit time {seconds:?} is not a time"))?
            .strftime("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let tags = git(repo, &["tag", "--merged", "HEAD", "--list", "v*"])?;
        let version = tags.lines().filter_map(release).max();
        Ok(Self {
            version: version.map(|v| v.to_string()),
            commit,
            date,
        })
    }
}

/// The version of a release tag: `v<major>.<minor>.<patch>[-<pre-release>]`, v1.0.0 or later
/// (tools/dev/version.sh; the Go fork's v0.x tags never count).
fn release(tag: &str) -> Option<semver::Version> {
    let v = semver::Version::parse(tag.strip_prefix('v')?).ok()?;
    (v.major >= 1 && v.build.is_empty()).then_some(v)
}

/// git's output for `args` in `repo`, trimmed.
fn git(repo: &Path, args: &[&str]) -> Result<String, Fail> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| fail!("git: {e}"))?;
    if !out.status.success() {
        return Err(fail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Installs the program with `cargo install --path crates/cli --locked <cargo_args>`, named
/// after the repository's [`Build`]; cargo's exit code.
///
/// # Errors
/// The repository is not a git checkout, or cargo does not start.
pub fn run(cargo_args: &[String]) -> Result<i32, Fail> {
    let root = crate::root();
    let build = Build::read(&root)?;
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.args(["install", "--path", "crates/cli", "--locked"])
        .args(cargo_args)
        .current_dir(&root)
        .env(env_var!("BUILD_COMMIT"), &build.commit)
        .env(env_var!("BUILD_DATE"), &build.date);
    let app = ssg_base::APP_NAME;
    match &build.version {
        Some(v) => {
            cmd.env(env_var!("BUILD_VERSION"), v);
            eprintln!("installing {app} v{v}-{} ({})", build.commit, build.date);
        }
        None => {
            cmd.env_remove(env_var!("BUILD_VERSION"));
            eprintln!(
                "no release tag is reachable from HEAD (`git fetch --tags`): installing {app} v{}-{}",
                env!("CARGO_PKG_VERSION"),
                build.commit
            );
        }
    }
    let status = cmd.status().map_err(|e| fail!("cargo install: {e}"))?;
    Ok(status.code().unwrap_or(1))
}
