//! The repository's tools, run with `cargo dev <command>` (an alias in .cargo/config.toml):
//! the acceptance harness of the Go comparison (the test sites, build manifests, structdiff and
//! its self-test, DEVELOPMENT.md "Acceptance harness"), the licence check, and the notices and
//! archives of a release (DEVELOPMENT.md "CI and releases").

use std::path::{Path, PathBuf};

pub mod difflib;
pub mod fnmatch;
pub mod html;
pub mod licence;
pub mod manifest;
pub mod notices;
pub mod package;
pub mod py;
pub mod selftest;
pub mod sites;
pub mod structdiff;
pub mod txtar;
pub mod url;

/// A failure: its message, printed by the command.
#[derive(Debug)]
pub struct Fail(pub String);

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Fail {}

/// A [`Fail`] with a formatted message.
#[macro_export]
macro_rules! fail {
    ($($t:tt)*) => {
        $crate::Fail(format!($($t)*))
    };
}

/// The repository root (this crate is crates/dev).
#[must_use]
pub fn root() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    dir.parent()
        .and_then(Path::parent)
        .unwrap_or(dir)
        .to_owned()
}
