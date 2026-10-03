//! Shared vocabulary of the workspace (docs/rust-port/REWRITE_PLAN.md §2.4): the program's name
//! ([`APP_NAME`], [`env_var!`]), typed ids and
//! [`IdVec`], [`PageKind`]/[`KindSet`], [`Value`]/[`Map`]/[`Params`]/[`Date`], the path
//! newtypes, URLs, anchors, inflection and title case, globs, date parsing, diagnostics,
//! [`Collate`] and [`Sink`].

#![forbid(unsafe_code)]

pub mod anchor;
pub mod diag;
pub mod glob;
pub mod id;
pub mod inflect;
pub mod kind;
pub mod params;
pub mod paths;
pub mod text;
pub mod time;
pub mod title;
pub mod url;
pub mod value;

pub use id::{
    FormatId, FrameId, IdVec, Idx, ImageOpId, LangIdx, MediaTypeId, PageId, ResourceId,
    TaxonomyIdx, TermIdx, TxnId,
};
pub use kind::{KindSet, PageKind};
pub use params::Params;
pub use paths::{ContentKey, OutputPath, Permalink, TermKey, UrlPath};
pub use time::{Clock, DateError, parse_date};
pub use value::{Date, Map, Value};

/// The program's name, as a literal (the one place it is written in the source code besides the
/// binary's name in `crates/cli/Cargo.toml`): the version line, the generator tag, the cache
/// directory and the prefix of the environment variables come from it.
#[macro_export]
macro_rules! app_name {
    () => {
        "fugo"
    };
}

/// The prefix of the environment variables the program reads and sets: [`APP_NAME`] in upper
/// case (a test checks it).
#[macro_export]
macro_rules! env_prefix {
    () => {
        "FUGO"
    };
}

/// The name of an environment variable of the program: `env_var!("TIMINGS")` is
/// `<PREFIX>_TIMINGS`, a `&'static str`.
#[macro_export]
macro_rules! env_var {
    ($name:literal) => {
        concat!($crate::env_prefix!(), "_", $name)
    };
}

/// The program's name ([`app_name!`]).
pub const APP_NAME: &str = app_name!();

/// The prefix of the program's environment variables ([`env_prefix!`]), without the `_`.
pub const ENV_PREFIX: &str = env_prefix!();

/// The program's version: `<PREFIX>_BUILD_VERSION` at compile time when it is set and not
/// empty (CI sets it from the release tag, `v<version>`; `tools/dev/version.py`), else the
/// workspace's `version`, which is that of builds not made from a tag (`0.0.0-DEV`).
pub const VERSION: &str = match option_env!(env_var!("BUILD_VERSION")) {
    Some(v) if !v.is_empty() => v,
    _ => env!("CARGO_PKG_VERSION"),
};

/// Compares strings in a language's collation order (implemented with ICU in
/// `ssg-locale`).
pub trait Collate: Send + Sync {
    /// The collation order of `a` and `b`.
    fn compare(&self, a: &str, b: &str) -> std::cmp::Ordering;
}

/// Byte order: the collation used when a language has none.
#[derive(Clone, Copy, Debug, Default)]
pub struct ByteOrder;

impl Collate for ByteOrder {
    fn compare(&self, a: &str, b: &str) -> std::cmp::Ordering {
        a.cmp(b)
    }
}

/// Where rendered files go: the publish directory, or memory for `serve` and tests.
pub trait Sink: Send + Sync {
    /// Writes (or replaces) the file at `path`.
    ///
    /// # Errors
    /// I/O errors of the destination.
    fn write(&self, path: &OutputPath, bytes: &[u8]) -> std::io::Result<()>;
    /// Whether a file was written at `path`.
    fn exists(&self, path: &OutputPath) -> bool;
    /// The bytes last written at `path`.
    ///
    /// # Errors
    /// No file at `path`, or I/O errors of the destination.
    fn read(&self, path: &OutputPath) -> std::io::Result<Vec<u8>>;
}

#[cfg(test)]
mod name_tests {
    #[test]
    fn env_prefix_is_the_upper_case_name() {
        assert_eq!(super::ENV_PREFIX, super::APP_NAME.to_uppercase());
        assert_eq!(
            env_var!("TIMINGS"),
            format!("{}_TIMINGS", super::ENV_PREFIX)
        );
    }
}
