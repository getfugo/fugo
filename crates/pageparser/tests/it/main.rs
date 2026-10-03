//! Integration tests of `ssg-pageparser` (the crate's single test binary, REWRITE_PLAN.md §2.2).
//! The oracle tests print their tallies (`cargo test -p ssg-pageparser -- --nocapture`).

mod assemble;
mod front_matter;
mod lexer;
mod support;
