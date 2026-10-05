//! The Go implementation's results: lines of `go test -bench` output,
//! `BenchmarkSanitize/Spaces-4   12345678   95.21 ns/op   8 B/op   1 allocs/op`, as the
//! benchmark's name (without the `-<GOMAXPROCS>` suffix) and its nanoseconds per call.

/// Every result in `output`, in order.
#[must_use]
pub fn parse(output: &str) -> Vec<(String, f64)> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let name = *fields.first()?;
            if !name.starts_with("Benchmark") {
                return None;
            }
            let at = fields.iter().position(|f| *f == "ns/op")?;
            let value = fields.get(at.checked_sub(1)?)?.parse::<f64>().ok()?;
            Some((strip_procs(name).to_owned(), value))
        })
        .collect()
}

/// `BenchmarkX/y-4` → `BenchmarkX/y` (Go appends `-<GOMAXPROCS>` when it is not 1).
fn strip_procs(name: &str) -> &str {
    match name.rsplit_once('-') {
        Some((base, procs)) if !procs.is_empty() && procs.bytes().all(|b| b.is_ascii_digit()) => {
            base
        }
        _ => name,
    }
}
