//! Where published files go: the publish directory ([`DiskSink`]) or memory ([`MemorySink`],
//! for `serve` and tests).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use ssg_base::Sink;
use ssg_base::gate::Gate;
use ssg_base::paths::OutputPath;

/// How many files a [`DiskSink`] writes at once: more threads creating files in the same
/// directories only wait for each other in the file system (a 10,000-page build writes ~20%
/// faster on Linux and ~25% on macOS with 3 than with 12 at once).
const WRITERS: usize = 3;

/// Writes files below a directory, creating parent directories as needed. A file is always
/// truncated and rewritten (modes follow the process umask).
///
/// It remembers the directories it has seen, and which of them it created: a file in a known
/// directory is written at once, and so is the first file of a directory below one it created
/// (which cannot exist yet), after creating it; any other file is tried first, and its
/// directory created only when missing. A fresh build so creates each directory with one
/// `mkdir`, and a build into an existing tree writes each file with one `open`. Clones share
/// what it knows and its limit of [`WRITERS`] writes at once.
#[derive(Clone, Debug)]
pub struct DiskSink {
    pub root: PathBuf,
    /// The directories seen, and whether this sink created them.
    dirs: Arc<DashMap<PathBuf, bool>>,
    /// At most [`WRITERS`] writes at once.
    gate: Arc<Gate>,
}

impl DiskSink {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            dirs: Arc::default(),
            gate: Arc::new(Gate::new(WRITERS)),
        }
    }

    /// The file system path of `path`.
    #[must_use]
    pub fn file_path(&self, path: &OutputPath) -> PathBuf {
        self.root.join(path.relative())
    }

    /// Creates `dir` and its missing parents, remembering the ones it created.
    fn create_dirs(&self, dir: &Path) -> io::Result<()> {
        match fs::create_dir(dir) {
            Ok(()) => {
                self.dirs.insert(dir.to_owned(), true);
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let Some(parent) = dir.parent() else {
                    return Err(e);
                };
                self.create_dirs(parent)?;
                match fs::create_dir(dir) {
                    Ok(()) => {
                        self.dirs.insert(dir.to_owned(), true);
                        Ok(())
                    }
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
                    Err(e) => Err(e),
                }
            }
            // A file in its place fails the write.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl Sink for DiskSink {
    fn write(&self, path: &OutputPath, bytes: &[u8]) -> io::Result<()> {
        let _pass = self.gate.enter();
        let file = self.file_path(path);
        let Some(dir) = file.parent() else {
            return fs::write(&file, bytes);
        };
        if self.dirs.contains_key(dir) {
            return fs::write(&file, bytes);
        }
        let new = dir
            .parent()
            .is_some_and(|p| self.dirs.get(p).is_some_and(|created| *created));
        if new {
            self.create_dirs(dir)?;
            return fs::write(&file, bytes);
        }
        match fs::write(&file, bytes) {
            Ok(()) => {
                self.dirs.entry(dir.to_owned()).or_insert(false);
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.create_dirs(dir)?;
                self.dirs.entry(dir.to_owned()).or_insert(false);
                fs::write(&file, bytes)
            }
            Err(e) => Err(e),
        }
    }

    fn exists(&self, path: &OutputPath) -> bool {
        self.file_path(path).is_file()
    }

    fn read(&self, path: &OutputPath) -> io::Result<Vec<u8>> {
        fs::read(self.file_path(path))
    }
}

/// Keeps published files in memory. Concurrent writers are fine; a second write of a path
/// replaces the first.
#[derive(Debug, Default)]
pub struct MemorySink {
    pub files: DashMap<OutputPath, Arc<[u8]>>,
}

impl MemorySink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The bytes at `path` (`/posts/index.html` or `posts/index.html`).
    #[must_use]
    pub fn get(&self, path: &str) -> Option<Arc<[u8]>> {
        self.files
            .get(&OutputPath::new(path))
            .map(|e| Arc::clone(e.value()))
    }

    /// The bytes at `path` as text, when they are UTF-8.
    #[must_use]
    pub fn text(&self, path: &str) -> Option<String> {
        self.get(path)
            .and_then(|b| String::from_utf8(b.to_vec()).ok())
    }

    /// Every path, sorted.
    #[must_use]
    pub fn paths(&self) -> Vec<OutputPath> {
        let mut paths: Vec<OutputPath> = self.files.iter().map(|e| e.key().clone()).collect();
        paths.sort();
        paths
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Writes every file below `dir` (for inspecting a memory build).
    ///
    /// # Errors
    /// I/O errors.
    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        let disk = DiskSink::new(dir);
        for path in self.paths() {
            if let Some(bytes) = self.get(path.as_str()) {
                disk.write(&path, &bytes)?;
            }
        }
        Ok(())
    }
}

impl Sink for MemorySink {
    fn write(&self, path: &OutputPath, bytes: &[u8]) -> io::Result<()> {
        self.files.insert(path.clone(), Arc::from(bytes));
        Ok(())
    }

    fn exists(&self, path: &OutputPath) -> bool {
        self.files.contains_key(path)
    }

    fn read(&self, path: &OutputPath) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .map(|e| e.value().to_vec())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_string()))
    }
}
