//! Integration tests of `ssg-cli` (the crate's single test binary, REWRITE_PLAN.md §2.2): the
//! binary run on small sites.

mod acceptance;
mod build;
mod check;
mod cli;
mod cms;
mod docs;
mod docs_data;
mod docs_site;
mod embedded;
#[cfg(feature = "npm")]
mod npm;
mod parity;
mod server;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ssg_testkit::txtar::Archive;

/// The binary with `args`, in `dir`, with an environment reduced to `PATH` plus `env`.
pub fn binary(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_fugo"));
    c.current_dir(dir).args(args).env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        c.env("PATH", path);
    }
    c.env("HOME", dir);
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().expect("run the binary")
}

pub fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

pub fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// `tests/it/<name>`.
pub fn fixture(name: &str) -> PathBuf {
    ssg_testkit::fixture::repo_dir()
        .join("crates/cli/tests/it")
        .join(name)
}

/// The txtar site `tests/it/<name>` written into a new temporary directory.
pub fn site(name: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    Archive::read(&fixture(name))
        .expect("read txtar")
        .write_to(tmp.path())
        .expect("write site");
    tmp
}

/// `text` with the directory `dir` written as `[site]`: its resolved path first (this port
/// reports resolved paths, and macOS's temporary directory, `/var/…`, resolves to
/// `/private/var/…`), then the path as given.
pub fn redact_site(text: &str, dir: &Path) -> String {
    let given = dir.display().to_string();
    let resolved = dir
        .canonicalize()
        .map_or_else(|_| given.clone(), |p| p.display().to_string());
    text.replace(&resolved, "[site]").replace(&given, "[site]")
}

/// A site from txtar text.
pub fn site_from(text: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    Archive::parse(text)
        .write_to(tmp.path())
        .expect("write site");
    tmp
}
