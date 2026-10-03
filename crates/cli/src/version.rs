//! The line `version` and `--version` print, in the Go build's format (`BuildVersionString` of
//! its `version.go` at 44529028):
//!
//! ```text
//! <name> v<version>[-<commit>] <os>/<arch> BuildDate=<date|unknown>[ VendorInfo=<vendor>]
//! ```
//!
//! `<version>` is [`VERSION`](crate::VERSION). The commit, the date and the vendor are set at
//! build time by `FUGO_BUILD_COMMIT` (Go: `vcs.revision`), `FUGO_BUILD_DATE` (Go: `vcs.time`,
//! the commit's time in UTC, RFC 3339) and `FUGO_VENDOR_INFO` (the releases: the program's name, as the
//! Go releases' ldflags set it); CI's release builds set all three. `<os>/<arch>` are Go's names
//! of the target (`linux`, `darwin`, `windows`; `amd64`, `arm64`).

use std::fmt;
use std::sync::LazyLock;

/// What the version line says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildInfo<'a> {
    /// [`VERSION`](crate::VERSION).
    pub version: &'a str,
    /// The commit the binary was built from.
    pub commit: Option<&'a str>,
    /// Go's `GOOS` name of the target.
    pub os: &'a str,
    /// Go's `GOARCH` name of the target.
    pub arch: &'a str,
    /// The commit's date (`unknown` when not set).
    pub date: Option<&'a str>,
    /// Who built the binary.
    pub vendor: Option<&'a str>,
}

impl BuildInfo<'static> {
    /// This binary's.
    pub const CURRENT: Self = Self {
        version: crate::VERSION,
        commit: non_empty(option_env!(ssg_base::env_var!("BUILD_COMMIT"))),
        os: GO_OS,
        arch: GO_ARCH,
        date: non_empty(option_env!(ssg_base::env_var!("BUILD_DATE"))),
        vendor: non_empty(option_env!(ssg_base::env_var!("VENDOR_INFO"))),
    };
}

impl fmt::Display for BuildInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} v{}", ssg_base::APP_NAME, self.version)?;
        if let Some(commit) = self.commit {
            write!(f, "-{commit}")?;
        }
        let date = self.date.unwrap_or("unknown");
        write!(f, " {}/{} BuildDate={date}", self.os, self.arch)?;
        if let Some(vendor) = self.vendor {
            write!(f, " VendorInfo={vendor}")?;
        }
        Ok(())
    }
}

/// [`BuildInfo::CURRENT`]'s line.
#[must_use]
pub fn line() -> &'static str {
    static LINE: LazyLock<String> = LazyLock::new(|| BuildInfo::CURRENT.to_string());
    &LINE
}

/// Go's `GOOS` (Rust's `OS`, but `darwin` for `macos`).
const GO_OS: &str = if cfg!(target_os = "macos") {
    "darwin"
} else {
    std::env::consts::OS
};

/// Go's `GOARCH` (Rust's `ARCH`, but `amd64`, `arm64` and `386` for `x86_64`, `aarch64` and `x86`).
const GO_ARCH: &str = if cfg!(target_arch = "x86_64") {
    "amd64"
} else if cfg!(target_arch = "aarch64") {
    "arm64"
} else if cfg!(target_arch = "x86") {
    "386"
} else {
    std::env::consts::ARCH
};

/// An unset or empty build-time variable is no value.
const fn non_empty(value: Option<&'static str>) -> Option<&'static str> {
    match value {
        Some(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}
