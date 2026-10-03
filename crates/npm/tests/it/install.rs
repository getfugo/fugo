//! `ensure_installed` against a local registry.

use std::path::Path;

use serde_json::{Value, json};
use ssg_npm::{InstallError, Installed, LOCK_FILE, ensure_installed};
use ssg_testkit::registry::{Package, Registry};

/// `shout` (CommonJS), `greet` (a program that depends on `shout`), `@scope/esm` (an ES module
/// program).
pub(crate) fn packages() -> Vec<Package> {
    vec![
        Package::new("shout", "1.0.0")
            .field("main", json!("index.js"))
            .file("index.js", "module.exports = (s) => s.toUpperCase() + '!';\n"),
        Package::new("shout", "1.2.0")
            .field("main", json!("index.js"))
            .file("index.js", "module.exports = (s) => s.toUpperCase() + '!!';\n"),
        Package::new("greet", "1.0.0")
            .field("bin", json!({ "greet": "bin/greet.js" }))
            .field("dependencies", json!({ "shout": "^1.0.0" }))
            .file(
                "bin/greet.js",
                r"#!/usr/bin/env node
const fs = require('node:fs');
const path = require('path');
const shout = require('shout');
const input = fs.readFileSync(0, 'utf8').trim();
const out = `${shout(input)} ${process.argv[2]} ${process.env.GREETING} ${path.basename(process.cwd())}`;
process.stdout.write(out + '\n');
fs.writeFileSync('out.txt', out);
",
            ),
        Package::new("@scope/esm", "2.0.0")
            .field("type", json!("module"))
            .field("bin", json!({ "esm": "cli.js", "fail": "fail.js" }))
            .field("dependencies", json!({ "shout": "~1.2.0" }))
            .file(
                "cli.js",
                r"import { readFileSync, writeFileSync } from 'node:fs';
import shout from 'shout';
writeFileSync('out.txt', shout(readFileSync(0, 'utf8').trim()));
process.exitCode = 3;
",
            )
            .file("fail.js", "throw new Error('the program failed');\n"),
    ]
}

/// A project with `package.json` and an `.npmrc` that points at `registry`.
pub(crate) fn project(registry: &Registry, package_json: &Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write_json(&dir.path().join("package.json"), package_json);
    std::fs::write(dir.path().join(".npmrc"), registry.npmrc()).expect(".npmrc");
    dir
}

fn write_json(path: &Path, v: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(v).expect("json")).expect("write");
}

fn version(project: &Path, package: &str) -> String {
    let path = project
        .join("node_modules")
        .join(package)
        .join("package.json");
    let json: Value = serde_json::from_slice(&std::fs::read(&path).expect("package.json"))
        .expect("parse package.json");
    json["version"].as_str().expect("version").to_owned()
}

#[test]
fn installs_the_dependencies_hoisted_and_writes_the_lock_file() {
    let registry = Registry::start(&packages());
    let cache = tempfile::tempdir().expect("cache");
    let p = project(
        &registry,
        &json!({ "devDependencies": { "greet": "^1.0.0" } }),
    );
    let first = ensure_installed(p.path(), cache.path()).expect("install");
    assert!(matches!(first, Installed::Installed { .. }), "{first:?}");
    assert_eq!(version(p.path(), "greet"), "1.0.0");
    // `greet`'s dependency is hoisted, at the highest version its range allows.
    assert_eq!(version(p.path(), "shout"), "1.2.0");
    let lock = std::fs::read_to_string(p.path().join(LOCK_FILE)).expect("lock file");
    assert!(lock.contains("\"npm:greet@1\": \"1.0.0\""), "{lock}");

    let requests = registry.requests().len();
    assert_eq!(
        ensure_installed(p.path(), cache.path()).expect("again"),
        Installed::UpToDate
    );
    assert_eq!(
        registry.requests().len(),
        requests,
        "nothing fetched when up to date"
    );
}

#[test]
fn installs_again_when_package_json_changes() {
    let registry = Registry::start(&packages());
    let cache = tempfile::tempdir().expect("cache");
    let p = project(&registry, &json!({ "dependencies": { "greet": "^1.0.0" } }));
    ensure_installed(p.path(), cache.path()).expect("install");
    write_json(
        &p.path().join("package.json"),
        &json!({ "dependencies": { "@scope/esm": "^2.0.0" } }),
    );
    let again = ensure_installed(p.path(), cache.path()).expect("install again");
    assert!(matches!(again, Installed::Installed { .. }), "{again:?}");
    assert_eq!(version(p.path(), "@scope/esm"), "2.0.0");
    // The dependency that left package.json left node_modules.
    assert!(!p.path().join("node_modules/greet").exists());
}

#[test]
fn the_lock_file_keeps_the_versions() {
    let all = packages();
    let registry = Registry::start(&all[..1]);
    let cache = tempfile::tempdir().expect("cache");
    let p = project(&registry, &json!({ "dependencies": { "shout": "^1.0.0" } }));
    ensure_installed(p.path(), cache.path()).expect("install");
    assert_eq!(version(p.path(), "shout"), "1.0.0");
    // A newer version is published; a new node_modules has the locked one.
    registry.publish(&all[1..2]);
    std::fs::remove_dir_all(p.path().join("node_modules")).expect("remove node_modules");
    ensure_installed(p.path(), tempfile::tempdir().expect("cache").path()).expect("reinstall");
    assert_eq!(version(p.path(), "shout"), "1.0.0");
    // Without the lock file (and the package documents cached), the range picks the newest.
    std::fs::remove_dir_all(p.path().join("node_modules")).expect("remove node_modules");
    std::fs::remove_file(p.path().join(LOCK_FILE)).expect("remove lock file");
    ensure_installed(p.path(), tempfile::tempdir().expect("cache").path()).expect("resolve again");
    assert_eq!(version(p.path(), "shout"), "1.2.0");
}

#[test]
fn nothing_to_install() {
    let registry = Registry::start(&[]);
    let cache = tempfile::tempdir().expect("cache");
    let p = project(&registry, &json!({ "name": "site", "private": true }));
    assert_eq!(
        ensure_installed(p.path(), cache.path()).expect("no dependencies"),
        Installed::NoPackages
    );
    std::fs::remove_file(p.path().join("package.json")).expect("remove");
    assert_eq!(
        ensure_installed(p.path(), cache.path()).expect("no package.json"),
        Installed::NoPackages
    );
    assert!(!p.path().join("node_modules").exists());
    assert!(registry.requests().is_empty());
}

#[test]
fn another_package_managers_node_modules_is_left_alone() {
    let registry = Registry::start(&packages());
    let cache = tempfile::tempdir().expect("cache");
    let p = project(&registry, &json!({ "dependencies": { "greet": "^1.0.0" } }));
    let node_modules = p.path().join("node_modules");
    std::fs::create_dir(&node_modules).expect("node_modules");
    std::fs::write(node_modules.join(".package-lock.json"), "{}").expect("npm state");
    assert_eq!(
        ensure_installed(p.path(), cache.path()).expect("npm's"),
        Installed::External("npm")
    );
    std::fs::remove_file(node_modules.join(".package-lock.json")).expect("remove");
    std::fs::create_dir(node_modules.join("left-pad")).expect("a package");
    assert_eq!(
        ensure_installed(p.path(), cache.path()).expect("unknown"),
        Installed::External("another package manager")
    );
    assert!(registry.requests().is_empty());
}

#[cfg(unix)]
#[test]
fn a_linked_node_modules_is_left_alone() {
    let registry = Registry::start(&packages());
    let cache = tempfile::tempdir().expect("cache");
    let p = project(&registry, &json!({ "dependencies": { "greet": "^1.0.0" } }));
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    std::os::unix::fs::symlink(elsewhere.path(), p.path().join("node_modules")).expect("link");
    assert_eq!(
        ensure_installed(p.path(), cache.path()).expect("link"),
        Installed::External("a link")
    );
}

#[test]
fn a_missing_package_is_an_error() {
    let registry = Registry::start(&packages());
    let cache = tempfile::tempdir().expect("cache");
    let p = project(
        &registry,
        &json!({ "dependencies": { "no-such-package": "^1.0.0" } }),
    );
    let err = ensure_installed(p.path(), cache.path()).expect_err("missing package");
    assert!(matches!(err, InstallError::Install { .. }), "{err:?}");
    assert!(err.to_string().contains("no-such-package"), "{err}");
}

#[test]
fn a_broken_package_json_is_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("package.json"), "{ not json").expect("write");
    let err = ensure_installed(dir.path(), dir.path()).expect_err("broken");
    assert!(matches!(err, InstallError::PackageJson { .. }), "{err:?}");
}
