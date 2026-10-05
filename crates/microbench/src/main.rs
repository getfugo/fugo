//! `ssg-microbench <results.json> [--go <go-test-output>] [--target-ms <ms>] [--runs <n>]`
//! (or `--go-test-args`, which prints the `go test` arguments of the cases' Go benchmarks):
//! runs every case of `ssg_microbench::cases` and writes nanoseconds per call (the median of
//! `--runs` measures, each of at least `--target-ms`) as github-action-benchmark's custom JSON,
//! adding the Go implementation's results from `--go` as `… (Go)`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use serde_json::{Value, json};
use ssg_microbench::{cases, go, harness};

struct Args {
    out: PathBuf,
    go: Option<PathBuf>,
    target: Duration,
    runs: usize,
}

fn args() -> Result<Args, String> {
    let mut out = None;
    let mut go = None;
    let mut target = Duration::from_millis(300);
    let mut runs = 3;
    let mut it = std::env::args_os().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || {
            it.next()
                .ok_or_else(|| format!("{} needs a value", arg.display()))
        };
        match arg.to_str() {
            Some("--go") => go = Some(PathBuf::from(value()?)),
            Some("--target-ms") => {
                let ms = value()?
                    .to_string_lossy()
                    .parse()
                    .map_err(|e| format!("--target-ms: {e}"))?;
                target = Duration::from_millis(ms);
            }
            Some("--runs") => {
                runs = value()?
                    .to_string_lossy()
                    .parse()
                    .map_err(|e| format!("--runs: {e}"))?;
            }
            _ if out.is_none() => out = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument {}", arg.display())),
        }
    }
    let out = out.ok_or("usage: ssg-microbench <results.json> [--go <go-test-output>] [--target-ms <ms>] [--runs <n>]")?;
    Ok(Args {
        out,
        go,
        target,
        runs,
    })
}

fn run(args: &Args) -> Result<(), String> {
    let mut results = Vec::new();
    for case in cases::all() {
        let ns = harness::median_ns_per_op(&*case.run, args.target, args.runs);
        println!("{:<45} {ns:>12.2} ns/op", case.name);
        results.push(json!({ "name": case.name, "unit": "ns/op", "value": round2(ns) }));
    }
    if let Some(path) = &args.go {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let parsed = go::parse(&text);
        if parsed.is_empty() {
            return Err(format!("{}: no Go benchmark results", path.display()));
        }
        for (name, ns) in parsed {
            println!("{:<45} {ns:>12.2} ns/op", format!("{name} (Go)"));
            results.push(
                json!({ "name": format!("{name} (Go)"), "unit": "ns/op", "value": round2(ns) }),
            );
        }
    }
    let text = serde_json::to_string_pretty(&Value::Array(results)).map_err(|e| e.to_string())?;
    std::fs::write(&args.out, text + "\n").map_err(|e| format!("{}: {e}", args.out.display()))
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn main() -> ExitCode {
    // `--go-test-args`: the `go test` arguments that run the cases' Go benchmarks, for CI.
    if std::env::args().nth(1).as_deref() == Some("--go-test-args") {
        println!("{}", cases::go_test_args().join(" "));
        return ExitCode::SUCCESS;
    }
    match args().and_then(|a| run(&a)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ssg-microbench: {e}");
            ExitCode::FAILURE
        }
    }
}
