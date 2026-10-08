//! The `npm` feature through the binary: a build installs the packages of `package.json`
//! (from the registry of `.npmrc`, a local one here) without Node.js on `PATH`, and `js_build`
//! bundles what the site imports from them; a `node_modules` that npm left out of date is
//! installed again; the server installs again when `package.json` or `package-lock.json`
//! changes.

use std::path::Path;

use serde_json::json;
use ssg_testkit::registry::{Package, Registry};

use crate::server::Running;
use crate::{binary, site_from, stderr, stdout};

/// A `PATH` without Node.js.
const NO_NODE: (&str, &str) = ("PATH", "/usr/bin:/bin");

const SITE: &str = "-- config.toml --\nbaseURL = \"https://example.org/\"\ntitle = \"Npm\"\n\
disableKinds = [\"taxonomy\", \"term\", \"sitemap\", \"rss\", \"robotsTXT\"]\n\
-- layouts/home.html --\n<html><head>\
{%- set js = get_asset(path=\"js/main.js\") | js_build | fingerprint -%}\
<script src=\"{{ js.rel_permalink }}\"></script></head><body></body></html>\n\
-- assets/js/main.js --\nimport greet from 'greet';\ndocument.title = greet('npm');\n";

/// `greet` (CommonJS, with a dependency) and `extra`.
fn packages() -> Vec<Package> {
    vec![
        Package::new("punctuation", "1.0.0")
            .field("main", json!("index.js"))
            .file("index.js", "module.exports = '!';\n"),
        Package::new("greet", "1.0.0")
            .field("main", json!("index.js"))
            .field("dependencies", json!({ "punctuation": "^1.0.0" }))
            .file("index.js", GREET_JS),
        Package::new("extra", "1.0.0").file("index.js", "module.exports = 1;\n"),
    ]
}

fn write_project(dir: &Path, registry: &str, dependencies: &serde_json::Value) {
    let package_json = json!({ "private": true, "devDependencies": dependencies });
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_vec_pretty(&package_json).expect("json"),
    )
    .expect("package.json");
    std::fs::write(dir.join(".npmrc"), format!("registry={registry}\n")).expect(".npmrc");
}

/// `greet`'s `index.js`.
const GREET_JS: &str = "const mark = require('punctuation');\n\
                        module.exports = (name) => 'greetings from ' + name + mark;\n";

/// A `node_modules` as npm leaves it: its state file and the packages
/// `(name, version, index.js)`.
fn npm_node_modules(site: &Path, packages: &[(&str, &str, &str)]) {
    let node_modules = site.join("node_modules");
    std::fs::create_dir_all(&node_modules).expect("node_modules");
    std::fs::write(node_modules.join(".package-lock.json"), "{}").expect("npm state");
    for (name, version, index) in packages {
        let dir = node_modules.join(name);
        std::fs::create_dir_all(&dir).expect("package");
        let manifest = json!({ "name": name, "version": version, "main": "index.js" });
        std::fs::write(dir.join("package.json"), manifest.to_string()).expect("package.json");
        std::fs::write(dir.join("index.js"), index).expect("index.js");
    }
}

/// The published bundle (`js/main.<hash>.js`).
fn bundle(site: &Path) -> String {
    let dir = site.join("public/js");
    let file = std::fs::read_dir(&dir)
        .expect("public/js")
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("main."))
        .expect("main.*.js");
    std::fs::read_to_string(file.path()).expect("read js")
}

#[test]
fn a_build_installs_package_json_without_node() {
    let registry = Registry::start(&packages());
    let s = site_from(SITE);
    write_project(s.path(), registry.url(), &json!({ "greet": "^1.0.0" }));

    let out = binary(s.path(), &["build"], &[NO_NODE]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("Installed the npm packages of package.json"),
        "{}",
        stdout(&out)
    );
    let js = bundle(s.path());
    assert!(js.contains("greetings from "), "{js}");
    assert!(s.path().join("npm.lock").is_file());
    assert!(s.path().join("node_modules/punctuation/index.js").is_file());

    // Installed already: nothing fetched, nothing printed.
    let requests = registry.requests().len();
    let again = binary(s.path(), &["build"], &[NO_NODE]);
    assert!(again.status.success(), "{}", stderr(&again));
    assert!(!stdout(&again).contains("Installed"), "{}", stdout(&again));
    assert_eq!(registry.requests().len(), requests);
}

#[test]
fn a_build_installs_again_the_node_modules_npm_left_out_of_date() {
    let registry = Registry::start(&packages());
    let s = site_from(SITE);
    write_project(s.path(), registry.url(), &json!({ "greet": "^1.0.0" }));
    // npm installed an older greet, before package.json moved on.
    npm_node_modules(
        s.path(),
        &[("greet", "0.1.0", "module.exports = () => 'out of date';\n")],
    );

    let out = binary(s.path(), &["build"], &[NO_NODE]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains(
            "the node_modules npm wrote was out of date \
             (greet 0.1.0 is installed, package.json wants ^1.0.0)"
        ),
        "{}",
        stdout(&out)
    );
    let js = bundle(s.path());
    assert!(js.contains("greetings from "), "{js}");
    assert!(!js.contains("out of date"), "{js}");
}

#[test]
fn a_registry_that_cannot_be_reached_fails_the_build() {
    let s = site_from(SITE);
    // Port 9 (discard) on the loopback address refuses connections.
    write_project(
        s.path(),
        "http://127.0.0.1:9/",
        &json!({ "greet": "^1.0.0" }),
    );
    let out = binary(s.path(), &["build"], &[NO_NODE]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("installing the npm packages of "), "{err}");
    assert!(err.contains("greet"), "{err}");
}

#[test]
fn the_server_installs_again_when_package_json_changes() {
    let registry = Registry::start(&packages());
    let s = site_from(SITE);
    write_project(s.path(), registry.url(), &json!({ "greet": "^1.0.0" }));
    let run = Running::start(s.path(), &["server", "-p", "0"]);
    let started = run.until("Web Server is available at ").join("\n");
    assert!(started.contains("Installed the npm packages"), "{started}");
    assert!(!s.path().join("node_modules/extra").exists());

    write_project(
        s.path(),
        registry.url(),
        &json!({ "greet": "^1.0.0", "extra": "^1.0.0" }),
    );
    run.until("Installed the npm packages");
    assert!(s.path().join("node_modules/extra/index.js").is_file());
}

#[test]
fn the_server_installs_again_when_package_lock_json_changes() {
    let registry = Registry::start(&packages());
    registry.publish(&[Package::new("punctuation", "1.0.1")
        .field("main", json!("index.js"))
        .file("index.js", "module.exports = '!!';\n")]);
    let s = site_from(SITE);
    write_project(s.path(), registry.url(), &json!({ "greet": "^1.0.0" }));
    // npm's node_modules has what package.json wants: left alone.
    npm_node_modules(
        s.path(),
        &[
            ("greet", "1.0.0", GREET_JS),
            ("punctuation", "1.0.0", "module.exports = '!';\n"),
        ],
    );
    let run = Running::start(s.path(), &["server", "-p", "0"]);
    let started = run.until("Web Server is available at ").join("\n");
    assert!(!started.contains("Installed"), "{started}");

    // A pull brings package-lock.json, with a newer punctuation.
    let locked = |name: &str, version: &str| {
        json!({
            "version": version,
            "resolved": format!("{}{name}/-/{name}-{version}.tgz", registry.url()),
            "integrity": registry.integrity(name, version).expect("integrity"),
            "dev": true,
        })
    };
    let mut greet = locked("greet", "1.0.0");
    greet["dependencies"] = json!({ "punctuation": "^1.0.0" });
    let lock = json!({
        "lockfileVersion": 3,
        "packages": {
            "": { "devDependencies": { "greet": "^1.0.0" } },
            "node_modules/greet": greet,
            "node_modules/punctuation": locked("punctuation", "1.0.1"),
        },
    });
    std::fs::write(
        s.path().join("package-lock.json"),
        serde_json::to_vec_pretty(&lock).expect("json"),
    )
    .expect("package-lock.json");
    let installed = run.until("Installed the npm packages").join("\n");
    assert!(
        installed.contains("(punctuation 1.0.0 is installed, package-lock.json has 1.0.1)"),
        "{installed}"
    );
    let punctuation =
        std::fs::read_to_string(s.path().join("node_modules/punctuation/package.json"))
            .expect("punctuation");
    assert!(punctuation.contains("\"1.0.1\""), "{punctuation}");
}
