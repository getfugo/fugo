//! Integration tests of `ssg-nav` (the crate's single test binary, REWRITE_PLAN.md §2.2).
//!
//! The oracle tests replay the Go fixtures of menus, pagination and related content under
//! `testdata/oracle/page/`, and the alias files of the Go builds under
//! `testdata/oracle/sitebuild/build/`, and print their pass rates.

mod aliases;
mod menus;
mod pagination;
mod related;
mod structure;
mod support;
mod units;
