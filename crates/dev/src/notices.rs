//! The third-party licence notices of a release build.
//!
//! The notices cover every package linked into the binary for a target: the normal-dependency
//! closure of the `ssg-cli` package in `cargo metadata --filter-platform <target>`. Build and
//! dev dependencies are not linked into the binary, and neither are proc-macro packages (they
//! run in the compiler), so the walk does not enter them. For each package the file lists its
//! name, version, licence expression, authors and where its source is, followed by the licence
//! and notice files the package ships: files in the package root named LICENSE*, LICENCE*,
//! COPYING*, NOTICE*, COPYRIGHT*, UNLICENSE*, AUTHORS* or PATENTS*, and for packages that build
//! C code (`links`, or a `-sys` name) the same files anywhere below the package, which covers
//! vendored C libraries such as libwebp. A text already printed for an earlier package is
//! referenced instead of repeated.
//!
//! A package that ships no licence file but is licensed under MIT (alone or as one choice) gets
//! the MIT licence text with the authors from its Cargo.toml; one under Apache-2.0 refers to
//! the Apache-2.0 text another linked package ships; one under a licence whose standard text is
//! in THIRD_PARTY/spdx/<SPDX id>.txt (BSD-3-Clause, CC0-1.0, BSL-1.0) gets that text, the
//! copyright holders being its authors. Any other package without a licence file is printed
//! and the run fails unless `--allow-missing` is given, so a new dependency like that is
//! noticed. Workspace members are covered by the repository's LICENSE.
//!
//! `package` puts the result into the release archives as THIRD_PARTY_NOTICES.txt
//! (.github/workflows/ci.yml, DEVELOPMENT.md "CI and releases").

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{Fail, fail};

const PREFIXES: [&str; 8] = [
    "license",
    "licence",
    "copying",
    "notice",
    "copyright",
    "unlicense",
    "authors",
    "patents",
];
const SKIP_DIRS: [&str; 7] = [
    ".git", "tests", "test", "benches", "examples", "target", "fuzz",
];
const MIT: &str = r#"Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software without
restriction, including without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
"#;

fn is_notice(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    path.is_file()
        && PREFIXES.iter().any(|p| name.starts_with(p))
        && ![
            ".rs", ".py", ".sh", ".toml", ".json", ".yml", ".yaml", ".c", ".h",
        ]
        .iter()
        .any(|e| name.ends_with(e))
}

/// The package's root and its licence and notice files.
fn notice_files(pkg: &Value) -> (PathBuf, Vec<PathBuf>) {
    let root = Path::new(pkg["manifest_path"].as_str().unwrap_or(""))
        .parent()
        .map(Path::to_owned)
        .unwrap_or_default();
    let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| is_notice(p))
        .collect();
    found.sort();
    let builds_c = pkg["links"].as_str().is_some_and(|l| !l.is_empty())
        || pkg["name"].as_str().is_some_and(|n| n.ends_with("-sys"));
    if builds_c {
        let mut below: Vec<PathBuf> = walkdir::WalkDir::new(&root)
            .min_depth(1)
            .into_iter()
            .filter_map(Result::ok)
            .map(walkdir::DirEntry::into_path)
            .collect();
        below.sort();
        for path in below {
            let rel = path.strip_prefix(&root).expect("below the root");
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            let skipped = parts[..parts.len() - 1]
                .iter()
                .any(|p| SKIP_DIRS.contains(&p.as_str()));
            if parts.len() > 1 && !skipped && is_notice(&path) {
                found.push(path);
            }
        }
    }
    (root, found)
}

fn is_proc_macro(pkg: &Value) -> bool {
    pkg["targets"].as_array().into_iter().flatten().any(|t| {
        t["kind"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|k| k == "proc-macro")
    })
}

/// Package ids linked into `target_pkg`: its normal-dependency closure without proc macros,
/// itself excluded.
fn closure(
    meta: &Value,
    packages: &HashMap<&str, &Value>,
    target_pkg: &str,
) -> Result<HashSet<String>, Fail> {
    let members: HashSet<&str> = meta["workspace_members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let nodes: HashMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| Some((n["id"].as_str()?, n)))
        .collect();
    let root = meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"] == target_pkg && p["id"].as_str().is_some_and(|i| members.contains(i)))
        .and_then(|p| p["id"].as_str())
        .ok_or_else(|| fail!("no workspace package {target_pkg}"))?;
    let (mut seen, mut todo) = (HashSet::new(), vec![root.to_owned()]);
    while let Some(id) = todo.pop() {
        for dep in nodes
            .get(id.as_str())
            .and_then(|n| n["deps"].as_array())
            .into_iter()
            .flatten()
        {
            let pid = dep["pkg"].as_str().unwrap_or("");
            let normal = dep["dep_kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|k| k["kind"].is_null());
            if normal && !seen.contains(pid) && !packages.get(pid).is_some_and(|p| is_proc_macro(p))
            {
                seen.insert(pid.to_owned());
                todo.push(pid.to_owned());
            }
        }
    }
    Ok(seen)
}

fn offers(expr: Option<&str>, licence: &str) -> bool {
    expr.unwrap_or("")
        .replace(['(', ')', '/'], " ")
        .split_whitespace()
        .any(|w| w == licence)
}

/// A text file as Python's text mode reads it: invalid UTF-8 replaced, newlines as `\n`;
/// trailing whitespace stripped and one newline added.
fn read_text(path: &Path) -> String {
    let raw = std::fs::read(path).unwrap_or_default();
    let text = String::from_utf8_lossy(&raw)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    format!("{}\n", text.trim_end_matches(crate::py::is_space))
}

fn rel_name(path: &Path, root: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn name_version(pkg: &Value) -> (String, String) {
    (
        pkg["name"].as_str().unwrap_or("").to_owned(),
        pkg["version"].as_str().unwrap_or("").to_owned(),
    )
}

/// The first Apache-2.0 licence file a linked package ships: (text, "<name> <version>, <file>").
fn apache_text(ids: &[String], packages: &HashMap<&str, &Value>) -> Option<(String, String)> {
    for pid in ids {
        let pkg = packages[pid.as_str()];
        let (root, files) = notice_files(pkg);
        for path in files {
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase().contains("apache"))
            {
                let text = read_text(&path);
                if text.contains("Apache License") && text.contains("Version 2.0") {
                    let (name, version) = name_version(pkg);
                    return Some((
                        text,
                        format!("{name} {version}, {}", rel_name(&path, &root)),
                    ));
                }
            }
        }
    }
    None
}

/// Writes the notices of `target` to `out`; the exit status (1 when a package ships no licence
/// file and its licence has no known text, unless `allow_missing`).
///
/// # Errors
/// Failing `cargo metadata` or a file that cannot be written.
pub fn run(target: &str, out: &Path, allow_missing: bool) -> Result<i32, Fail> {
    let meta = crate::licence::cargo_metadata(&["--filter-platform", target])?;
    let packages: HashMap<&str, &Value> = meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p["id"].as_str()?, p)))
        .collect();
    let members: HashSet<&str> = meta["workspace_members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let app = meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["name"] == "ssg-cli")
        .flat_map(|p| p["targets"].as_array().into_iter().flatten())
        .find(|t| {
            t["kind"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|k| k == "bin")
        })
        .and_then(|t| t["name"].as_str())
        .ok_or_else(|| fail!("ssg-cli has no binary"))?
        .to_owned();
    let mut ids: Vec<String> = closure(&meta, &packages, "ssg-cli")?
        .into_iter()
        .filter(|i| !members.contains(i.as_str()))
        .collect();
    ids.sort_by_key(|i| name_version(packages[i.as_str()]));

    let spdx_dir = crate::root().join("THIRD_PARTY/spdx");
    let mut spdx: Vec<PathBuf> = std::fs::read_dir(&spdx_dir)
        .map_err(|e| fail!("{}: {e}", spdx_dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    spdx.sort();

    let mut missing = Vec::new();
    let mut printed: HashMap<String, String> = HashMap::new(); // licence text -> where it was printed
    let title = format!("Third-party software in {app}");
    let mut parts = vec![
        format!("{title}\n"),
        format!("{}\n\n", "=".repeat(title.chars().count())),
        format!(
            "{app} for {target} links the {} packages below. Their licence and notice\n",
            ids.len()
        ),
        "files follow each entry. The source code of every package is available from\n".to_owned(),
        "https://crates.io/crates/<name>/<version> and from the repository listed with it.\n"
            .to_owned(),
        "Files copied into the source tree (data, fonts, scripts) are listed in\n".to_owned(),
        "PROVENANCE.md, with their licences in THIRD_PARTY/.\n".to_owned(),
    ];
    for pid in &ids {
        let pkg = packages[pid.as_str()];
        let (name, version) = name_version(pkg);
        let expr = pkg["license"].as_str();
        let (root, files) = notice_files(pkg);
        parts.push(format!("\n{}\n", "=".repeat(78)));
        parts.push(format!("{name} {version}\n"));
        let shown = expr
            .filter(|e| !e.is_empty())
            .or_else(|| pkg["license_file"].as_str().filter(|f| !f.is_empty()))
            .unwrap_or("unknown");
        parts.push(format!("License: {shown}\n"));
        let authors: Vec<&str> = pkg["authors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !authors.is_empty() {
            parts.push(format!("Authors: {}\n", authors.join(", ")));
        }
        parts.push(format!(
            "Source: https://crates.io/crates/{name}/{version}\n"
        ));
        if let Some(repo) = pkg["repository"].as_str().filter(|r| !r.is_empty()) {
            parts.push(format!("Repository: {repo}\n"));
        }
        if files.is_empty() {
            let apache = if offers(expr, "Apache-2.0") {
                apache_text(&ids, &packages)
            } else {
                None
            };
            if offers(expr, "MIT") {
                parts.push(format!(
                    "\nThe package ships no licence file. It is licensed under MIT (one of the choices above); the MIT licence text follows, the copyright holders being its authors listed above.\n\n{MIT}"
                ));
            } else if let Some((text, source)) = apache {
                let place = if printed.contains_key(&text) {
                    "above"
                } else {
                    "here"
                };
                parts.push(format!(
                    "\nThe package ships no licence file. It is licensed under Apache-2.0; the licence text is the same as {source}, printed {place}.\n"
                ));
                if let Entry::Vacant(e) = printed.entry(text) {
                    parts.push(format!("\n{}", e.key()));
                    e.insert(source);
                }
            } else if let Some(file) = spdx
                .iter()
                .find(|t| offers(expr, &t.file_stem().unwrap_or_default().to_string_lossy()))
            {
                let text = read_text(file);
                let stem = file
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let source = format!(
                    "THIRD_PARTY/spdx/{}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                );
                let tail = if printed.contains_key(&text) {
                    format!("is {source}, printed above.\n")
                } else {
                    "follows.\n".to_owned()
                };
                parts.push(format!(
                    "\nThe package ships no licence file. It is licensed under {stem}; the copyright holders being its authors listed above, the licence text {tail}"
                ));
                if let Entry::Vacant(e) = printed.entry(text) {
                    parts.push(format!("\n{}", e.key()));
                    e.insert(source);
                }
            } else {
                missing.push(format!("{name} {version} ({})", expr.unwrap_or("None")));
                parts.push(
                    "\n(the package ships no licence file; its licence is the expression above)\n"
                        .to_owned(),
                );
            }
        }
        for path in &files {
            let rel = rel_name(path, &root);
            let text = read_text(path);
            if let Some(source) = printed.get(&text) {
                parts.push(format!(
                    "\n--- {rel}: the same text as {source} above ---\n"
                ));
            } else {
                parts.push(format!("\n--- {rel} ---\n\n{text}"));
                printed.insert(text, format!("{name} {version}, {rel}"));
            }
        }
    }
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| fail!("{}: {e}", dir.display()))?;
    }
    let text = parts.concat();
    std::fs::write(out, &text).map_err(|e| fail!("{}: {e}", out.display()))?;
    println!(
        "notices: {} packages, {} bytes -> {}",
        ids.len(),
        text.len(),
        out.display()
    );
    if !missing.is_empty() {
        eprintln!("notices: no licence file in:\n  {}", missing.join("\n  "));
        if !allow_missing {
            return Ok(1);
        }
    }
    Ok(0)
}
