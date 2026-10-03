//! The Go implementation's import resolution for `js.Build`: imports are looked up in the assets
//! filesystem before the bundler's own resolver sees them.
//!
//! An import path is resolved as an asset when the importer is the entry script or itself an
//! asset (a file under one of the assets mounts): relative imports are resolved against the
//! importer's directory, other imports against the assets root. [`resolve_component`] then
//! tries, in order: the path with `.js`, `.ts`, `.tsx`, `.jsx` appended; for an `index` import
//! the `index.esm.*` variants; the path itself (a directory gives its `index.*`, then
//! `index.esm.*`); and for a `.js` import the same path without `.js`. Anything not found is
//! left to the bundler (which looks in `node_modules`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The extensions tried for an import without one, in order.
const EXTENSIONS: [&str; 4] = [".js", ".ts", ".tsx", ".jsx"];

/// What an assets path names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetEntry {
    /// A file, with its real filename.
    File(PathBuf),
    Dir,
}

/// The assets filesystem, as seen by the import resolver. Builds run on the bundler's threads,
/// so the view is shared (`Send + Sync`).
pub trait Assets: Send + Sync {
    /// The entry at a `/`-separated path relative to the assets root (no leading `/`).
    fn entry(&self, path: &str) -> Option<AssetEntry>;

    /// The assets path of a real (absolute) filename, when it lies under an assets mount.
    fn assets_path(&self, filename: &Path) -> Option<String>;
}

/// An [`Assets`] of directories mounted into the assets root; the first mount that has an
/// entry wins.
#[derive(Clone, Debug, Default)]
pub struct MountedDirs {
    mounts: Vec<(PathBuf, String)>,
}

impl MountedDirs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mounts the directory `source` at `target` (`""` for the assets root, `vendor` for
    /// `assets/vendor`).
    #[must_use]
    pub fn mount(mut self, source: impl Into<PathBuf>, target: &str) -> Self {
        self.mounts
            .push((source.into(), target.trim_matches('/').to_owned()));
        self
    }
}

impl Assets for MountedDirs {
    fn entry(&self, path: &str) -> Option<AssetEntry> {
        let path = clean(path);
        self.mounts.iter().find_map(|(source, target)| {
            let rest = if target.is_empty() {
                path.as_str()
            } else if path == *target {
                ""
            } else {
                path.strip_prefix(target.as_str())?.strip_prefix('/')?
            };
            if rest == ".." || rest.starts_with("../") {
                return None;
            }
            let full = if rest.is_empty() || rest == "." {
                source.clone()
            } else {
                source.join(rest)
            };
            let meta = std::fs::metadata(&full).ok()?;
            Some(if meta.is_dir() {
                AssetEntry::Dir
            } else {
                AssetEntry::File(full)
            })
        })
    }

    fn assets_path(&self, filename: &Path) -> Option<String> {
        self.mounts.iter().find_map(|(source, target)| {
            let rel = filename.strip_prefix(source).ok()?;
            let rel = rel.to_str()?.replace('\\', "/");
            Some(match (target.is_empty(), rel.is_empty()) {
                (true, _) => rel,
                (false, true) => target.clone(),
                (false, false) => format!("{target}/{rel}"),
            })
        })
    }
}

/// Resolves an import path to an asset file (see the module docs for the order).
pub fn resolve_component(imp: &str, entry: impl Fn(&str) -> Option<AssetEntry>) -> Option<PathBuf> {
    let file = |p: &str| match entry(p) {
        Some(AssetEntry::File(f)) => Some(f),
        _ => None,
    };
    // `foo.js.js` must be imported by its full name: an extension the import already has is
    // not appended again.
    let first = |base: &str| {
        EXTENSIONS
            .iter()
            .filter(|ext| !imp.ends_with(*ext))
            .find_map(|ext| file(&format!("{base}{ext}")))
    };

    if let Some(f) = first(imp) {
        return Some(f);
    }
    let base = imp.rsplit('/').next().unwrap_or(imp);
    if base == "index"
        && let Some(f) = first(&format!("{imp}.esm"))
    {
        return Some(f);
    }
    match entry(imp) {
        Some(AssetEntry::File(f)) => return Some(f),
        Some(AssetEntry::Dir) => {
            let index = first(&join(imp, "index")).or_else(|| first(&join(imp, "index.esm")));
            if index.is_some() {
                return index;
            }
        }
        None => {}
    }
    base.strip_suffix(".js")
        .and(imp.strip_suffix(".js"))
        .and_then(first)
}

/// [`resolve_component`] over [`Assets`], memoized per import path for one build.
pub(crate) struct ComponentResolver {
    assets: Arc<dyn Assets>,
    cache: Mutex<HashMap<String, Option<PathBuf>>>,
}

impl ComponentResolver {
    pub(crate) fn new(assets: Arc<dyn Assets>) -> Self {
        Self {
            assets,
            cache: Mutex::default(),
        }
    }

    pub(crate) fn resolve(&self, imp: &str) -> Option<PathBuf> {
        if let Some(hit) = lock(&self.cache).get(imp) {
            return hit.clone();
        }
        let found = resolve_component(imp, |p| self.assets.entry(p));
        lock(&self.cache).insert(imp.to_owned(), found.clone());
        found
    }

    pub(crate) fn assets(&self) -> &dyn Assets {
        &*self.assets
    }
}

impl std::fmt::Debug for ComponentResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentResolver").finish_non_exhaustive()
    }
}

/// A poisoned memo is still a valid memo.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The directory of a `/`-separated path (`.` for a bare name).
pub(crate) fn dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => ".",
    }
}

/// Joins two `/`-separated paths and cleans the result.
pub(crate) fn join(a: &str, b: &str) -> String {
    if a.is_empty() {
        clean(b)
    } else {
        clean(&format!("{a}/{b}"))
    }
}

/// Removes `.` segments, empty segments and `x/..` pairs; `..` above a relative root is kept.
pub(crate) fn clean(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => match out.last() {
                Some(&last) if last != ".." => {
                    out.pop();
                }
                _ if absolute => {}
                _ => out.push(".."),
            },
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}
