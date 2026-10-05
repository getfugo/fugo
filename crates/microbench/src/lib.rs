//! Micro-benchmarks of fugo's functions, matched to the Go implementation's. Each case is a Go
//! benchmark of fugo up to 0.148 (a `func Benchmark…` of the Go tree, at its last commit
//! `44529028` or before it was removed) with the same input, run on the Rust function that does
//! the same job, under the name Go gave it (`BenchmarkSanitize/Spaces`).
//!
//! `cargo run --release --locked -p ssg-microbench -- <results.json> [--go <go-test-output>]`
//! writes nanoseconds per call as github-action-benchmark's custom JSON; with `--go`, it adds the
//! Go implementation's results from the output of `go test -bench` in the same run, named
//! `… (Go)`. CI runs both on every push to `main`; the Benchmarks page of the documentation draws
//! each case on the chart of its Go benchmark's history (DEVELOPMENT.md, "Benchmarks").

pub mod cases;
pub mod go;
pub mod harness;
