//! The template API: [`spec`] (the single source of truth: `spec::FUNCS`, the render contexts and
//! `spec::EMBEDDED_TEMPLATES`), [`register_placeholders`] for the contract instance, and (feature
//! `runtime`) [`register_pure`]: the pure Tera filters, functions and tests plus the tera-contrib
//! subset.
//!
//! [`scan`] tokenizes template tags for the static checks Tera does not make (kwargs, the lints
//! of `templates check`).
//!
//! `spec` and `scan` have no dependencies and `register_placeholders` needs only tera, so all are available
//! with `default-features = false` (ssg-testkit's contract test uses that; REWRITE_PLAN.md §4.8).
//!
//! # Attaching the functions to a Tera instance
//!
//! The build's single Tera instance (REWRITE_PLAN.md §4.1) calls, before any template is added:
//!
//! 1. [`register_pure`] with a [`PureEnv`] (clock, time zone, site languages, title and anchor
//!    styles, path options, diagnostics sink, `security` rules) — every pure `FUNCS` entry;
//! 2. `ssg_sitefuncs::register` — every site-bound entry. A later registration under the same
//!    name replaces an earlier one, so sitefuncs may also refine a pure entry.
//!
//! The contract instance calls [`register_placeholders`] instead.

#![forbid(unsafe_code)]

mod placeholders;
pub mod scan;
pub mod spec;

#[cfg(feature = "runtime")]
mod pure;

pub use placeholders::{check_kwargs, register_placeholders};
#[cfg(feature = "runtime")]
pub use pure::{
    EnvAllowlist, Locales, NOT_COMPILED, PureEnv, date_value, pure_specs, register_pure,
};
