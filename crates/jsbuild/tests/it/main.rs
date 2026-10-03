//! Integration tests of `ssg-jsbuild` (the crate's single test binary, REWRITE_PLAN.md §2.2).
//!
//! Tests that run scripts use `node` on `PATH`; without it they print `SKIPPED` and pass.

mod build;
mod css;
mod decorators;
mod es5;
mod jsbuild;
mod options;
mod resolve;
mod run;

use std::path::{Path, PathBuf};

/// The repository root.
fn repo_root() -> PathBuf {
    ssg_testkit::fixture::repo_dir()
}

/// A fresh scratch directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("ssg-jsbuild-{}", std::process::id()))
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.canonicalize().expect("scratch dir")
}
