//! Integration tests of `ssg-page` (the crate's single test binary, REWRITE_PLAN.md §2.2).
//!
//! The oracle tests replay the Go fixtures under `testdata/oracle/page/` that concern
//! single pages (target paths, permalinks, dates, cascade, markup, build options, the default
//! sort order) and print their pass rates; menus, pagination and related content are
//! `ssg-nav`'s.

mod collections;
mod frontmatter;
mod misc;
mod paths;
mod permalinks;
mod support;
mod units;
