//! Integration tests of `ssg-microbench`.

use std::time::Duration;

use ssg_microbench::{cases, go, harness};

/// Every case does its work and passes the checks its Go benchmark makes.
#[test]
fn every_case_runs() {
    let all = cases::all();
    assert!(all.len() >= 10);
    for case in &all {
        (case.run)();
        assert!(case.name.starts_with("Benchmark"), "{}", case.name);
    }
    let mut names: Vec<_> = all.iter().map(|c| c.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), all.len(), "case names are unique");
}

#[test]
fn timing_is_per_call() {
    let ns = harness::ns_per_op(
        &|| std::thread::sleep(Duration::from_micros(200)),
        Duration::from_millis(5),
    );
    assert!(ns >= 200_000.0, "{ns}");
}

#[test]
fn go_test_output() {
    let out = "goos: linux\nBenchmarkSanitize/All_allowed-4   \t100000000\t        10.52 ns/op\t       0 B/op\t       0 allocs/op\nBenchmarkTotalWords  \t  5000\t 2345 ns/op\nPASS\nok  \tgithub.com/x/y\t3.2s\n";
    assert_eq!(
        go::parse(out),
        vec![
            ("BenchmarkSanitize/All_allowed".to_owned(), 10.52),
            ("BenchmarkTotalWords".to_owned(), 2345.0),
        ]
    );
}

#[test]
fn go_test_args_name_each_benchmark_once() {
    let args = cases::go_test_args();
    assert!(
        args[0].starts_with("-bench=^(") && args[0].ends_with(")$"),
        "{}",
        args[0]
    );
    assert!(args[0].contains("|BenchmarkSanitize|"), "{}", args[0]);
    assert!(
        !args[0].contains("BenchmarkStripHTML"),
        "removed from the Go tree before 44529028"
    );
    assert!(args.contains(&"./helpers".to_owned()));
    assert_eq!(args.iter().filter(|a| *a == "./helpers").count(), 1);
}
