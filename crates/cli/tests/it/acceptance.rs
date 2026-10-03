//! The acceptance harness from a test: `tools/dev/compare.sh <label> --ref golden` with the
//! test's binary against the committed golden data of the Go build, and the parsed
//! `structdiff.json` (REWRITE_PLAN.md §7.2, §7.3). The Go binaries are not needed.
//!
//! The sites' scripts import node modules (`tools/dev/node.sh`, which compare.sh links into each
//! site as its `node_modules`); without them, or without `python3` and `bash`, [`compare`]
//! prints `SKIPPED` and returns `None`.

use std::process::Command;

use ssg_testkit::fixture::repo_dir;

/// `program --version` runs.
fn runs(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Whether the tools are there (the node modules `tools/dev/node.sh` installs); `None` prints
/// `SKIPPED <gate>`.
fn tools(gate: &str) -> Option<()> {
    let skip = |why: String| {
        eprintln!("SKIPPED {gate}: {why}");
        None
    };
    if !runs("python3") || !runs("bash") {
        return skip("python3 and bash are needed on PATH".to_owned());
    }
    let Some(node_modules) = ssg_testkit::fixture::node_tools() else {
        return skip("tools/dev/node.sh does not run".to_owned());
    };
    if !node_modules.join(".lock-sha256").is_file() {
        return skip(format!(
            "no node modules in {} (run tools/dev/node.sh)",
            node_modules.display()
        ));
    }
    Some(())
}

/// Runs `compare.sh <label> --ref golden` (ratchet against `testdata/baselines/<label>.json`)
/// and returns its `structdiff.json`; `None` when the tools are missing (`SKIPPED <gate>`).
///
/// # Panics
/// When compare.sh fails (an unlisted difference, or a build error), or its output is missing.
pub fn compare(gate: &str, label: &str) -> Option<serde_json::Value> {
    tools(gate)?;
    let repo = repo_dir().canonicalize().expect("repository root");
    let work = tempfile::tempdir().expect("tempdir");
    let out = Command::new("bash")
        .arg(repo.join("tools/dev/compare.sh"))
        .args([label, "--ref", "golden", "--show", "20"])
        .env("FUGO_BINARY", env!("CARGO_BIN_EXE_fugo"))
        .env("FUGO_COMPARE_WORK", work.path())
        .env_remove("FUGO_TASK")
        .env_remove("KEEP")
        .output()
        .expect("run compare.sh");
    let report = String::from_utf8_lossy(&out.stdout);
    let log = String::from_utf8_lossy(&out.stderr);
    println!("{report}");
    assert!(
        out.status.success(),
        "compare.sh failed (an unlisted difference, or a build error):\n{report}\n{log}"
    );
    let json = std::fs::read_to_string(work.path().join(label).join("structdiff.json"))
        .expect("structdiff.json");
    let d: serde_json::Value = serde_json::from_str(&json).expect("structdiff.json parses");
    assert_eq!(d["ref"], "golden");
    assert_eq!(d["ratchet"]["verdict"], "pass");
    Some(d)
}

/// L1 of both passes: `files` matched, none missing or extra.
pub fn assert_l1(d: &serde_json::Value, files: u64) {
    assert_l1_passes(d, &["minified", "unminified"], files);
}

/// L1 of each of `passes`: `files` matched, none missing or extra.
pub fn assert_l1_passes(d: &serde_json::Value, passes: &[&str], files: u64) {
    for pass in passes {
        let p = &d["passes"][pass];
        assert_eq!(
            (
                p["matched"].as_u64(),
                p["missing"].as_u64(),
                p["extra"].as_u64()
            ),
            (Some(files), Some(0), Some(0)),
            "L1 {pass}: {p}"
        );
    }
}

/// Every compared file or fact of each of `levels` is equal.
pub fn assert_equal(d: &serde_json::Value, levels: &[&str]) {
    for level in levels {
        let s = &d["summary"][level];
        assert_eq!(
            s["ok"], s["compared"],
            "{level} must be equal everywhere: {s}"
        );
    }
}
