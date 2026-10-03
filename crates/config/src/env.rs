//! The environment variables the configuration reads: `HOME`, `XDG_CACHE_HOME`, `TMPDIR` and
//! `USER`, for the default cache directory. Settings and the build environment never come from
//! the environment: they come from the configuration files and the command line (`--environment`;
//! the command chooses the default), and the variables templates read come from the project's
//! `.env` files ([`crate::env_file`]).

/// The prefix of the variables the program reads and sets (`FUGO`).
pub const PREFIX: &str = ssg_base::ENV_PREFIX;

/// Whether the configuration reads the variable `name` ([`crate::LoadOptions::env`]).
#[must_use]
pub fn is_read(name: &str) -> bool {
    matches!(name, "HOME" | "XDG_CACHE_HOME" | "TMPDIR" | "USER")
}
