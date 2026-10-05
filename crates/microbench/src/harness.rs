//! Timing as Go's `testing.B` does it: a case runs `n` times, `n` growing until one run takes at
//! least the target time, and the time per call is that run's time over `n`. Each case is
//! measured several times; the median counts.

use std::time::{Duration, Instant};

/// Nanoseconds per call of `f`, from runs of at least `target`.
#[must_use]
pub fn ns_per_op(f: &dyn Fn(), target: Duration) -> f64 {
    let mut n: u64 = 1;
    loop {
        let start = Instant::now();
        for _ in 0..n {
            f();
        }
        let elapsed = start.elapsed();
        if elapsed >= target || n >= 1 << 40 {
            return elapsed.as_secs_f64() * 1e9 / n as f64;
        }
        // Go's `predictN`: aim 20% past the target, grow at least by one and at most 100 times.
        let per_op = (elapsed.as_secs_f64() / n as f64).max(1e-9);
        let next = (target.as_secs_f64() * 1.2 / per_op) as u64;
        n = next.clamp(n + 1, n.saturating_mul(100));
    }
}

/// The median of `runs` measures of `f`.
#[must_use]
pub fn median_ns_per_op(f: &dyn Fn(), target: Duration, runs: usize) -> f64 {
    let mut values: Vec<f64> = (0..runs.max(1)).map(|_| ns_per_op(f, target)).collect();
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}
