//! Internal crate that every workspace member depends on (docs/rust-port/REWRITE_PLAN.md §2.2).
//!
//! Cargo unifies features per invocation over the selected packages only, so `cargo test -p a`
//! and `cargo test -p b` would build light shared dependencies with different feature sets and
//! keep both artifacts in the shared target directory. This crate depends on those dependencies
//! with the union of the features the workspace enables (see its `Cargo.toml`), so every `-p`
//! build sees the same set. cargo 1.94.1 has no stable `resolver.feature-unification`.
//!
//! Heavy crates never go here. The list is checked with `cargo tree -e features`; see
//! `DEVELOPMENT.md` ("Feature unification").

#![forbid(unsafe_code)]
