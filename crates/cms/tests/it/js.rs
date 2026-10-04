//! The JavaScript tests of the editor and the Worker (`tests/js/*.test.js`), run with
//! `node --test`. Without node on `PATH` the test prints `SKIPPED` (CI fails on that).

use std::path::{Path, PathBuf};
use std::process::Command;

fn node() -> Option<PathBuf> {
    let found = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(if cfg!(windows) { "node.exe" } else { "node" }))
            .find(|f| f.is_file())
    });
    if found.is_none() {
        eprintln!("SKIPPED cms js tests: no node on PATH");
    }
    found
}

#[test]
fn javascript_tests_pass() {
    let Some(node) = node() else {
        return;
    };
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut tests: Vec<PathBuf> = std::fs::read_dir(crate_dir.join("tests/js"))
        .expect("tests/js")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.to_str().is_some_and(|s| s.ends_with(".test.js")))
        .collect();
    tests.sort();
    assert!(!tests.is_empty(), "no tests/js/*.test.js");
    // A Worker as a build publishes it (worker.test.js loads it and calls its default export).
    let tmp = tempfile::tempdir().expect("tempdir");
    let published = tmp.path().join("_worker.mjs");
    std::fs::write(
        &published,
        ssg_cms::worker_source(&published_settings(), r#"{"title":"Snacks","entries":[]}"#),
    )
    .expect("write _worker.mjs");
    let out = Command::new(node)
        .arg("--test")
        .args(&tests)
        .current_dir(crate_dir)
        .env("CMS_PUBLISHED_WORKER", &published)
        .output()
        .expect("node --test");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    println!("{report}");
    assert!(out.status.success(), "node --test failed:\n{report}");
}

/// Settings for the published Worker of the JavaScript tests: one role, `owner`.
fn published_settings() -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "path": "/admin/",
        "api": "/admin/api/",
        "site": "http://localhost:8787/",
        "workflow": "review",
        "git": {"host": "github", "repo": "owner/site", "branch": "main", "dir": ""},
        "login": {"provider": "cloudflare-access", "team": "https://team.cloudflareaccess.com", "aud": ["aud"]},
        "roles": {"owner": {"edit": ["**"], "publish": true}},
        "areas": [],
        "deny": ["**/_content.*"],
        "maxUpload": 1_048_576
    })
}
