//! Copying the static files (phase E1): the union view of the static mounts is copied to the
//! publish root before anything is rendered, so rendered outputs win conflicts.
//!
//! [`sync_static_dir`] mirrors Go's sync of the publish directory: a file is rewritten only
//! when it is missing or its bytes differ, permissions and modification times are copied from
//! the source (unless `noChmod` / `noTimes`), and with `cleanDestinationDir` every file of the
//! publish directory that is not a static file is removed first (directories whose name starts
//! with `.`, such as `.git`, are kept). [`sync_static`] copies the same files into any
//! [`Sink`] (memory builds).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use ssg_base::paths::{self, OutputPath};
use ssg_base::{IdVec, Idx, LangIdx, Sink, Value};
use ssg_config::Config;
use ssg_vfs::{Component, FileRef, NFC_NAMES, Vfs, entry_name};

use crate::PublishError;

/// How [`sync_static_dir`] treats the publish directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticSyncOptions {
    /// Copy modification times (`noTimes = false`).
    pub times: bool,
    /// Copy permissions (`noChmod = false`).
    pub permissions: bool,
    /// Remove what is not a static file first (`cleanDestinationDir`).
    pub clean_destination: bool,
    /// Multihost sites: each language's static files go below its own directory, indexed by
    /// language (`None`: one tree at the root).
    pub language_dirs: Option<IdVec<LangIdx, String>>,
}

impl Default for StaticSyncOptions {
    fn default() -> Self {
        Self {
            times: true,
            permissions: true,
            clean_destination: false,
            language_dirs: None,
        }
    }
}

impl StaticSyncOptions {
    /// The options of a configuration (`noTimes`, `noChmod`, `cleanDestinationDir`, and the
    /// language directories of a multihost site).
    #[must_use]
    pub fn from_config(cfg: &Config) -> Self {
        let flag = |key: &str| cfg.raw.get(key).and_then(Value::as_bool).unwrap_or(false);
        let language_dirs = cfg.multihost.then(|| {
            let mut dirs = IdVec::with_capacity(cfg.sites.len());
            for s in &cfg.sites {
                dirs.push(s.language.key.clone());
            }
            dirs
        });
        Self {
            times: !flag("notimes"),
            permissions: !flag("nochmod"),
            clean_destination: flag("cleandestinationdir"),
            language_dirs,
        }
    }

    /// The publish path of a static file (below its language's directory on a multihost site;
    /// no leading slash).
    #[must_use]
    pub fn target(&self, f: &FileRef) -> String {
        match &self.language_dirs {
            Some(dirs) => {
                let lang = f.mount_lang.unwrap_or_else(|| LangIdx::from_index(0));
                let dir = dirs.get(lang).map_or("", String::as_str);
                paths::join(&[dir, &f.rel])
            }
            None => f.rel.clone(),
        }
    }
}

/// Copies every static file into `sink`; returns the number of files.
///
/// # Errors
/// An unreadable mount or file, or a failing sink.
pub fn sync_static(
    vfs: &Vfs,
    sink: &dyn Sink,
    o: &StaticSyncOptions,
) -> Result<usize, PublishError> {
    let files = vfs.walk(Component::Static)?;
    files.par_iter().try_for_each(|f| {
        let bytes = fs::read(&f.abs).map_err(|e| PublishError::io(&f.abs, e))?;
        let path = OutputPath::new(&o.target(f));
        sink.write(&path, &bytes)
            .map_err(|source| PublishError::Write { path, source })
    })?;
    Ok(files.len())
}

/// Synchronises the publish directory `root` with the static files; returns the number of
/// static files.
///
/// # Errors
/// An unreadable mount or file, or a file system error in `root`.
pub fn sync_static_dir(
    vfs: &Vfs,
    root: &Path,
    o: &StaticSyncOptions,
) -> Result<usize, PublishError> {
    let files = vfs.walk(Component::Static)?;
    let targets: Vec<(String, &FileRef)> = files.iter().map(|f| (o.target(f), f)).collect();

    // The directories of the targets, each with the source directory it mirrors (none for the
    // directories a mount's target adds).
    let mut dirs: BTreeMap<String, Option<PathBuf>> = BTreeMap::new();
    for (target, f) in &targets {
        let mount_depth = vfs.mounts()[usize::from(f.mount_idx)]
            .target_dir()
            .split('/')
            .filter(|s| !s.is_empty())
            .count();
        let lang_depth = target.split('/').count() - f.rel.split('/').count();
        let mut dest = paths::dir(target);
        let mut src = f.abs.parent();
        let mut depth = dest.split('/').filter(|s| !s.is_empty()).count();
        while depth > 0 {
            let mirrored = (depth > mount_depth + lang_depth)
                .then(|| src.map(Path::to_path_buf))
                .flatten();
            let entry = dirs.entry(dest.to_owned()).or_insert(None);
            if entry.is_none() {
                *entry = mirrored;
            }
            dest = paths::dir(dest);
            if dest == "/" || dest == "." {
                dest = "";
            }
            src = src.and_then(Path::parent);
            depth -= 1;
        }
    }

    fs::create_dir_all(root).map_err(|e| PublishError::io(root, e))?;
    for dir in dirs.keys() {
        make_dir(&root.join(dir))?;
    }
    targets
        .par_iter()
        .try_for_each(|(target, f)| sync_file(&f.abs, &root.join(target), o))?;

    if o.clean_destination {
        let keep: BTreeSet<PathBuf> = targets
            .iter()
            .map(|(t, _)| PathBuf::from(t))
            .chain(dirs.keys().map(PathBuf::from))
            .collect();
        clean(root, Path::new(""), Path::new(""), &keep, NFC_NAMES)?;
    }
    if o.times {
        // Deepest first, so that setting a directory's time is not undone by its children.
        for (dir, src) in dirs.iter().rev() {
            if let Some(src) = src {
                copy_mtime(src, &root.join(dir))?;
            }
        }
    }
    Ok(targets.len())
}

/// Creates `dir`, replacing a file that is in the way (of `dir` or of one of its parents).
fn make_dir(dir: &Path) -> Result<(), PublishError> {
    match fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => return Ok(()),
        Ok(_) => fs::remove_file(dir).map_err(|e| PublishError::io(dir, e))?,
        Err(_) => {
            if let Some(parent) = dir.parent() {
                make_dir(parent)?;
            }
        }
    }
    match fs::create_dir(dir) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => Err(PublishError::io(dir, e)),
        _ => Ok(()),
    }
}

fn sync_file(src: &Path, dest: &Path, o: &StaticSyncOptions) -> Result<(), PublishError> {
    let bytes = fs::read(src).map_err(|e| PublishError::io(src, e))?;
    let current = match fs::symlink_metadata(dest) {
        Ok(m) if m.is_dir() => {
            fs::remove_dir_all(dest).map_err(|e| PublishError::io(dest, e))?;
            None
        }
        Ok(m) if m.len() == bytes.len() as u64 => fs::read(dest).ok(),
        _ => None,
    };
    if current.as_deref() != Some(bytes.as_slice()) {
        fs::write(dest, &bytes).map_err(|e| PublishError::io(dest, e))?;
    }
    if o.permissions {
        let perms = fs::metadata(src)
            .map_err(|e| PublishError::io(src, e))?
            .permissions();
        fs::set_permissions(dest, perms).map_err(|e| PublishError::io(dest, e))?;
    }
    if o.times {
        copy_mtime(src, dest)?;
    }
    Ok(())
}

fn copy_mtime(src: &Path, dest: &Path) -> Result<(), PublishError> {
    let meta = fs::metadata(src).map_err(|e| PublishError::io(src, e))?;
    let mtime = filetime::FileTime::from_last_modification_time(&meta);
    filetime::set_file_mtime(dest, mtime).map_err(|e| PublishError::io(dest, e))
}

/// Removes what is below `root/os_rel` and not in `keep` (paths relative to `root`, with the
/// static files' names); directories whose name starts with `.` are left alone. `rel` is
/// `os_rel` with the build's names: the names the publish directory lists are compared with
/// `keep` as [`entry_name`] gives them (NFC when `nfc`, on macOS, where the static files' names
/// are NFC and the publish directory may hold their NFD form), and removed by the OS's name.
fn clean(
    root: &Path,
    os_rel: &Path,
    rel: &Path,
    keep: &BTreeSet<PathBuf>,
    nfc: bool,
) -> Result<(), PublishError> {
    let dir = root.join(os_rel);
    let entries = fs::read_dir(&dir).map_err(|e| PublishError::io(&dir, e))?;
    let mut names: Vec<(PathBuf, PathBuf, bool)> = Vec::new();
    for e in entries {
        let e = e.map_err(|e| PublishError::io(&dir, e))?;
        let is_dir = e
            .file_type()
            .map_err(|err| PublishError::io(e.path(), err))?
            .is_dir();
        let os_name = e.file_name();
        let name = match os_name.to_str() {
            Some(n) => PathBuf::from(entry_name(n, nfc).into_owned()),
            None => PathBuf::from(&os_name),
        };
        names.push((rel.join(name), os_rel.join(os_name), is_dir));
    }
    names.sort();
    for (path, os_path, is_dir) in names {
        let hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        let abs = root.join(&os_path);
        if is_dir {
            if hidden {
                continue;
            }
            if keep.contains(&path) {
                clean(root, &os_path, &path, keep, nfc)?;
            } else {
                fs::remove_dir_all(&abs).map_err(|e| PublishError::io(&abs, e))?;
            }
        } else if !keep.contains(&path) {
            fs::remove_file(&abs).map_err(|e| PublishError::io(&abs, e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::clean;

    /// On macOS the static files' names are NFC while the publish directory may list them
    /// decomposed (HFS+, or an earlier copy under the source's NFD name): `cleanDestinationDir`
    /// keeps them; without normalisation the NFD directory is not a static file's.
    #[test]
    fn clean_compares_nfc_names() {
        let root = tempfile::tempdir().expect("temp dir");
        let nfd = root.path().join("cafe\u{301}");
        fs::create_dir(&nfd).expect("mkdir");
        fs::write(nfd.join("logo.png"), b"png").expect("write");
        fs::write(nfd.join("stale.png"), b"old").expect("write");
        let keep: BTreeSet<PathBuf> = ["caf\u{e9}", "caf\u{e9}/logo.png"]
            .into_iter()
            .map(PathBuf::from)
            .collect();
        clean(root.path(), Path::new(""), Path::new(""), &keep, true).expect("clean");
        assert!(nfd.join("logo.png").is_file());
        assert!(!nfd.join("stale.png").exists());
        clean(root.path(), Path::new(""), Path::new(""), &keep, false).expect("clean");
        assert!(!nfd.exists());
    }
}
