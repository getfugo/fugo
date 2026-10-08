//! [`out_of_date`]: whether a `node_modules` that another package manager wrote has the
//! packages of `package.json` and, for npm's, the versions of `package-lock.json`.

use std::path::Path;

use deno_semver::{Version, VersionReq};
use serde_json::Value;

/// Why `<project>/node_modules` does not have the packages the project names, or `None` when
/// it has them: the first package that is missing, or installed at a version that its range in
/// `package.json` does not allow. With `npm_lock`, the versions of npm's `package-lock.json`
/// count too.
pub(crate) fn out_of_date(project: &Path, package_json: &Value, npm_lock: bool) -> Option<String> {
    declared(project, package_json).or_else(|| npm_lock.then(|| locked(project)).flatten())
}

/// The first dependency of `package.json` that `node_modules` lacks or has at a version its
/// range does not allow. Not checked: what is not a range (a tag such as `latest`, a `file:`,
/// git or alias specifier) and a missing optional dependency (one for another platform).
fn declared(project: &Path, package_json: &Value) -> Option<String> {
    for (field, optional) in [
        ("dependencies", false),
        ("devDependencies", false),
        ("optionalDependencies", true),
    ] {
        let Some(deps) = package_json.get(field).and_then(Value::as_object) else {
            continue;
        };
        for (name, spec) in deps {
            let Some(spec) = spec.as_str() else { continue };
            let Some(req) = VersionReq::parse_from_npm(spec)
                .ok()
                .filter(|r| r.tag().is_none())
            else {
                continue;
            };
            match installed(&project.join("node_modules").join(name)) {
                None if optional => {}
                None => return Some(format!("{name} is not installed")),
                Some(v) if !allows(&req, &v) => {
                    return Some(format!(
                        "{name} {v} is installed, package.json wants {spec}"
                    ));
                }
                Some(_) => {}
            }
        }
    }
    None
}

/// The first package of npm's `package-lock.json` (lockfile versions 2 and 3: its `packages`)
/// that `node_modules` lacks or has at another version. Not checked: links (workspaces, `file:`
/// dependencies) and a missing optional package.
fn locked(project: &Path) -> Option<String> {
    let text = std::fs::read(project.join("package-lock.json")).ok()?;
    let lock: Value = serde_json::from_slice(&text).ok()?;
    for (path, entry) in lock.get("packages")?.as_object()? {
        // The project itself is "", a workspace member its directory.
        let Some(name) = path.strip_prefix("node_modules/") else {
            continue;
        };
        let flag = |key: &str| entry.get(key).and_then(Value::as_bool) == Some(true);
        if flag("link") {
            continue;
        }
        let Some(want) = entry.get("version").and_then(Value::as_str) else {
            continue;
        };
        match installed(&project.join(path)) {
            None if flag("optional") || flag("devOptional") => {}
            None => return Some(format!("{name} is not installed")),
            Some(v) if v != want => {
                return Some(format!(
                    "{name} {v} is installed, package-lock.json has {want}"
                ));
            }
            Some(_) => {}
        }
    }
    None
}

/// The version of the package installed in `dir`, if any.
fn installed(dir: &Path) -> Option<String> {
    let text = std::fs::read(dir.join("package.json")).ok()?;
    let json: Value = serde_json::from_slice(&text).ok()?;
    json.get("version")?.as_str().map(str::to_owned)
}

/// Whether `range` allows `version` (one that does not parse is not allowed).
fn allows(range: &VersionReq, version: &str) -> bool {
    Version::parse_from_npm(version).is_ok_and(|v| range.matches(&v))
}
