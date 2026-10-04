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

use crate::metadata::{Metadata, Package};
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
const NOT_NOTICES: [&str; 9] = [
    ".rs", ".py", ".sh", ".toml", ".json", ".yml", ".yaml", ".c", ".h",
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
        && !NOT_NOTICES.iter().any(|e| name.ends_with(e))
}

/// The package's root and its licence and notice files.
fn notice_files(pkg: &Package) -> (PathBuf, Vec<PathBuf>) {
    let root: PathBuf = pkg
        .manifest_path
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
    let builds_c =
        pkg.links.as_deref().is_some_and(|l| !l.is_empty()) || pkg.name.ends_with("-sys");
    if builds_c {
        let below = walkdir::WalkDir::new(&root)
            .min_depth(2)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                !(e.file_type().is_dir() && SKIP_DIRS.iter().any(|d| e.file_name() == *d))
            })
            .filter_map(Result::ok)
            .map(walkdir::DirEntry::into_path)
            .filter(|p| is_notice(p));
        found.extend(below);
    }
    (root, found)
}

/// The packages linked into the workspace package `name`: its normal-dependency closure
/// without proc macros, itself excluded.
fn closure<'m>(meta: &'m Metadata, name: &str) -> Result<HashSet<&'m String>, Fail> {
    let packages: HashMap<&String, &Package> = meta.packages.iter().map(|p| (&p.id, p)).collect();
    let nodes: HashMap<&String, _> = meta.resolve.nodes.iter().map(|n| (&n.id, n)).collect();
    let root = meta
        .member(name)
        .ok_or_else(|| fail!("no workspace package {name}"))?;
    let (mut seen, mut todo) = (HashSet::new(), vec![&root.id]);
    while let Some(id) = todo.pop() {
        for dep in nodes.get(id).into_iter().flat_map(|n| &n.deps) {
            let normal = dep.dep_kinds.iter().any(|k| k.kind.is_none());
            if normal
                && !packages
                    .get(&dep.pkg)
                    .is_some_and(|p| p.has_target("proc-macro"))
                && seen.insert(&dep.pkg)
            {
                todo.push(&dep.pkg);
            }
        }
    }
    Ok(seen)
}

/// Whether the licence expression names `licence` (as one of its terms).
fn offers(expr: Option<&str>, licence: &str) -> bool {
    expr.unwrap_or("")
        .replace(['(', ')', '/'], " ")
        .split_whitespace()
        .any(|w| w == licence)
}

/// A text file with invalid UTF-8 replaced, newlines as `\n`, trailing whitespace stripped
/// and one newline added.
fn read_text(path: &Path) -> String {
    let raw = std::fs::read(path).unwrap_or_default();
    let text = String::from_utf8_lossy(&raw)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    format!("{}\n", text.trim_end())
}

fn rel_name(path: &Path, root: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The first Apache-2.0 licence file a linked package ships: (text, "<name> <version>, <file>").
fn apache_text(linked: &[&Package]) -> Option<(String, String)> {
    linked.iter().find_map(|pkg| {
        let (root, files) = notice_files(pkg);
        files.into_iter().find_map(|path| {
            let named = path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase().contains("apache"));
            let text = if named { read_text(&path) } else { return None };
            (text.contains("Apache License") && text.contains("Version 2.0")).then(|| {
                (
                    text,
                    format!("{} {}, {}", pkg.name, pkg.version, rel_name(&path, &root)),
                )
            })
        })
    })
}

/// The licence texts already printed, and where.
#[derive(Default)]
struct Printed(HashMap<String, String>);

impl Printed {
    /// `text` with its source, unless it was printed before.
    fn first(&mut self, text: String, source: String) -> Option<String> {
        match self.0.entry(text) {
            Entry::Vacant(e) => {
                let text = e.key().clone();
                e.insert(source);
                Some(text)
            }
            Entry::Occupied(_) => None,
        }
    }
}

/// Writes the notices of `target` to `out`; the exit status (1 when a package ships no licence
/// file and its licence has no known text, unless `allow_missing`).
///
/// # Errors
/// Failing `cargo metadata` or a file that cannot be written.
pub fn run(target: &str, out: &Path, allow_missing: bool) -> Result<i32, Fail> {
    let meta = Metadata::read(&["--filter-platform", target])?;
    let members: HashSet<&String> = meta.workspace_members.iter().collect();
    let app = meta
        .member("ssg-cli")
        .into_iter()
        .flat_map(|p| &p.targets)
        .find(|t| t.kind.iter().any(|k| k == "bin"))
        .map(|t| t.name.clone())
        .ok_or_else(|| fail!("ssg-cli has no binary"))?;
    let ids = closure(&meta, "ssg-cli")?;
    let mut linked: Vec<&Package> = meta
        .packages
        .iter()
        .filter(|p| ids.contains(&p.id) && !members.contains(&p.id))
        .collect();
    linked.sort_by(|a, b| a.key().cmp(&b.key()));

    let spdx_dir = crate::root().join("THIRD_PARTY/spdx");
    let mut spdx: Vec<PathBuf> = std::fs::read_dir(&spdx_dir)
        .map_err(|e| fail!("{}: {e}", spdx_dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    spdx.sort();

    let mut missing = Vec::new();
    let mut printed = Printed::default();
    let title = format!("Third-party software in {app}");
    let mut parts = vec![
        format!("{title}\n{}\n\n", "=".repeat(title.chars().count())),
        format!(
            "{app} for {target} links the {} packages below. Their licence and notice\n",
            linked.len()
        ),
        "files follow each entry. The source code of every package is available from\n".to_owned(),
        "https://crates.io/crates/<name>/<version> and from the repository listed with it.\n"
            .to_owned(),
        "Files copied into the source tree (data, fonts, scripts) are listed in\n".to_owned(),
        "PROVENANCE.md, with their licences in THIRD_PARTY/.\n".to_owned(),
    ];
    for pkg in &linked {
        let (name, version) = (&pkg.name, &pkg.version);
        let expr = pkg.license.as_deref().filter(|e| !e.is_empty());
        let (root, files) = notice_files(pkg);
        parts.push(format!("\n{}\n{name} {version}\n", "=".repeat(78)));
        let shown = expr.map(str::to_owned).or_else(|| pkg.license_file.clone());
        parts.push(format!(
            "License: {}\n",
            shown.as_deref().unwrap_or("unknown")
        ));
        if !pkg.authors.is_empty() {
            parts.push(format!("Authors: {}\n", pkg.authors.join(", ")));
        }
        parts.push(format!(
            "Source: https://crates.io/crates/{name}/{version}\n"
        ));
        if let Some(repo) = pkg.repository.as_deref().filter(|r| !r.is_empty()) {
            parts.push(format!("Repository: {repo}\n"));
        }
        if files.is_empty() {
            let apache = if offers(expr, "Apache-2.0") {
                apache_text(&linked)
            } else {
                None
            };
            let standard = spdx
                .iter()
                .find(|t| offers(expr, &t.file_stem().unwrap_or_default().to_string_lossy()));
            if offers(expr, "MIT") {
                parts.push(format!(
                    "\nThe package ships no licence file. It is licensed under MIT (one of the choices above); the MIT licence text follows, the copyright holders being its authors listed above.\n\n{MIT}"
                ));
            } else if let Some((text, source)) = apache {
                let first = printed.first(text, source.clone());
                let place = if first.is_some() { "here" } else { "above" };
                parts.push(format!(
                    "\nThe package ships no licence file. It is licensed under Apache-2.0; the licence text is the same as {source}, printed {place}.\n"
                ));
                parts.extend(first.map(|t| format!("\n{t}")));
            } else if let Some(file) = standard {
                let stem = file.file_stem().unwrap_or_default().to_string_lossy();
                let source = format!(
                    "THIRD_PARTY/spdx/{}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                );
                let first = printed.first(read_text(file), source.clone());
                let tail = if first.is_some() {
                    "follows.\n".to_owned()
                } else {
                    format!("is {source}, printed above.\n")
                };
                parts.push(format!(
                    "\nThe package ships no licence file. It is licensed under {stem}; the copyright holders being its authors listed above, the licence text {tail}"
                ));
                parts.extend(first.map(|t| format!("\n{t}")));
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
            match printed.0.get(&text) {
                Some(source) => parts.push(format!(
                    "\n--- {rel}: the same text as {source} above ---\n"
                )),
                None => {
                    parts.push(format!("\n--- {rel} ---\n\n{text}"));
                    printed.0.insert(text, format!("{name} {version}, {rel}"));
                }
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
        linked.len(),
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
