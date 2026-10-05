//! `cargo dev bench`: measures a fugo binary. It builds fugo's documentation site and two
//! generated sites (`site.rs`, 1,000 and 10,000 pages), once to warm the caches and then `runs`
//! times, and reports the median build time and the median peak memory of each. The results are
//! the custom JSON of github-action-benchmark (`customSmallerIsBetter`): CI adds them to the
//! history on the `gh-pages` branch, which the documentation's Benchmarks page draws
//! (DEVELOPMENT.md, "Benchmarks").
//!
//! With `--go <binary>`, the generated sites are also built by the Go implementation (fugo up to
//! 0.148, built from the repository's history), from the same content with Go templates
//! (`templates.rs`): the results named `… (Go)` are its, measured on the same machine in the
//! same run, so the charts compare the two.
//!
//! Peak memory is the maximum resident set size `/usr/bin/time` reports (`-f %M` of GNU time on
//! Linux, `-l` on macOS); without it only times are measured.

mod site;
mod templates;

pub use site::generate;
pub use templates::Templates;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use serde_json::{Value, json};

use crate::{Fail, fail};

const TIME: &str = "/usr/bin/time";

/// One site to build: the binary, the arguments after `build` and the environment.
struct Site {
    name: String,
    /// ` (Go)` for the Go implementation's builds.
    suffix: String,
    binary: PathBuf,
    args: Vec<OsString>,
    env: Vec<(&'static str, PathBuf)>,
    publish: PathBuf,
}

/// The measures of one build.
struct Measure {
    millis: f64,
    /// Peak resident memory, MiB.
    rss: Option<f64>,
}

pub fn run(binary: &Path, out: &Path, runs: usize, go: Option<&Path>) -> Result<i32, Fail> {
    if runs == 0 {
        return Err(fail!("--runs must be at least 1"));
    }
    let canonical = |b: &Path| std::fs::canonicalize(b).map_err(|e| fail!("{}: {e}", b.display()));
    let binary = canonical(binary)?;
    let go = go.map(canonical).transpose()?;
    let work = tempfile::tempdir().map_err(|e| fail!("temporary directory: {e}"))?;
    let memory = Path::new(TIME).is_file();
    if !memory {
        eprintln!("bench: no {TIME}: measuring build times only");
    }

    let mut sites = vec![docs_site(&binary, work.path())];
    for pages in [1_000, 10_000] {
        let name = format!("{} generated pages", thousands(pages));
        let mut flavours = vec![(
            Templates::Tera,
            binary.clone(),
            String::new(),
            "--cache-dir",
        )];
        if let Some(go) = &go {
            flavours.push((Templates::Go, go.clone(), " (Go)".to_owned(), "--cacheDir"));
        }
        for (templates, binary, suffix, cache) in flavours {
            let key = format!("generated-{pages}-{templates:?}").to_lowercase();
            let dir = work.path().join(&key);
            generate(&dir, pages, templates)?;
            sites.push(Site {
                name: name.clone(),
                suffix,
                binary,
                args: build_args(&dir, work.path(), &key, cache),
                env: Vec::new(),
                publish: work.path().join(format!("{key}-public")),
            });
        }
    }

    let mut results = Vec::new();
    for site in &sites {
        // The warm-up build fills the caches (processed images) and counts the pages.
        let pages = warm_up(site)?;
        let mut measures = Vec::with_capacity(runs);
        for _ in 0..runs {
            measures.push(measure(site, memory)?);
        }
        let millis = median(measures.iter().map(|m| m.millis).collect());
        let builds = format!("median of {runs} builds after a warm-up build");
        let extra = pages.map_or(builds.clone(), |p| format!("{p} pages; {builds}"));
        let label = format!("{}{}", site.name, site.suffix);
        println!("{label:<30} {millis:>9.0} ms");
        results.push(json!({
            "name": format!("{}: build time{}", site.name, site.suffix),
            "unit": "ms",
            "value": round1(millis),
            "extra": extra,
        }));
        let rss: Vec<f64> = measures.iter().filter_map(|m| m.rss).collect();
        if rss.len() == measures.len() {
            let rss = median(rss);
            println!("{label:<30} {rss:>9.1} MiB");
            results.push(json!({
                "name": format!("{}: peak memory{}", site.name, site.suffix),
                "unit": "MiB",
                "value": round1(rss),
                "extra": extra,
            }));
        }
    }

    let text =
        serde_json::to_string_pretty(&Value::Array(results)).map_err(|e| fail!("results: {e}"))?;
    std::fs::write(out, text + "\n").map_err(|e| fail!("{}: {e}", out.display()))?;
    Ok(0)
}

/// fugo's documentation site, built as tools/docs/build.sh builds it.
fn docs_site(binary: &Path, work: &Path) -> Site {
    let docs = crate::root().join("docs");
    Site {
        name: "docs site".to_owned(),
        suffix: String::new(),
        binary: binary.to_owned(),
        args: build_args(&docs, work, "docs", "--cache-dir"),
        env: vec![("FUGO_RESOURCEDIR", work.join("docs-resources"))],
        publish: work.join("docs-public"),
    }
}

/// `-s <source> -d <work>/<name>-public <cache> <work>/<name>-cache` (`--cache-dir`, or the Go
/// implementation's `--cacheDir`).
fn build_args(source: &Path, work: &Path, name: &str, cache: &str) -> Vec<OsString> {
    vec![
        "-s".into(),
        source.into(),
        "-d".into(),
        work.join(format!("{name}-public")).into(),
        cache.into(),
        work.join(format!("{name}-cache")).into(),
    ]
}

fn command(site: &Site) -> Command {
    let mut cmd = Command::new(&site.binary);
    cmd.arg("build").args(&site.args);
    for (k, v) in &site.env {
        cmd.env(k, v);
    }
    cmd
}

/// Builds `site` once, without `--quiet`, and returns the page count fugo's summary line prints
/// (`pages 120 | …`; the Go implementation prints a table instead).
fn warm_up(site: &Site) -> Result<Option<String>, Fail> {
    let out = command(site)
        .output()
        .map_err(|e| fail!("{}: {e}", site.binary.display()))?;
    if !out.status.success() {
        return Err(fail!(
            "building the {}: {}\n{}",
            site.name,
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let pages = text
        .split_whitespace()
        .skip_while(|w| *w != "pages")
        .nth(1)
        .filter(|n| n.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_owned);
    Ok(pages)
}

/// One build of `site` into an emptied publish directory, timed, under `/usr/bin/time` when
/// `memory`.
fn measure(site: &Site, memory: bool) -> Result<Measure, Fail> {
    if site.publish.exists() {
        std::fs::remove_dir_all(&site.publish)
            .map_err(|e| fail!("{}: {e}", site.publish.display()))?;
    }
    let mut cmd = if memory {
        let mut c = Command::new(TIME);
        if cfg!(target_os = "macos") {
            c.arg("-l");
        } else {
            c.args(["-f", "maxrss-kib %M"]);
        }
        c.arg(&site.binary).arg("build").args(&site.args);
        for (k, v) in &site.env {
            c.env(k, v);
        }
        c
    } else {
        command(site)
    };
    cmd.arg("--quiet")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let start = Instant::now();
    let out = cmd
        .output()
        .map_err(|e| fail!("{}: {e}", site.binary.display()))?;
    let millis = start.elapsed().as_secs_f64() * 1000.0;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        return Err(fail!(
            "building the {}: {}\n{stderr}",
            site.name,
            out.status
        ));
    }
    let rss = if memory { peak_rss(&stderr) } else { None };
    Ok(Measure { millis, rss })
}

/// The peak resident set size in MiB from `/usr/bin/time`'s report: `maxrss-kib <KiB>` (the
/// format given to GNU time) or macOS's `<bytes>  maximum resident set size`.
pub fn peak_rss(report: &str) -> Option<f64> {
    report.lines().find_map(|line| {
        let line = line.trim();
        if let Some(kib) = line.strip_prefix("maxrss-kib ") {
            return kib.trim().parse::<f64>().ok().map(|k| k / 1024.0);
        }
        let bytes = line.strip_suffix("maximum resident set size")?;
        bytes
            .trim()
            .parse::<f64>()
            .ok()
            .map(|b| b / 1024.0 / 1024.0)
    })
}

pub fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if !n.is_multiple_of(2) {
        values[n / 2]
    } else {
        f64::midpoint(values[n / 2 - 1], values[n / 2])
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// `10000` → `10,000`.
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
