//! Where published files go: the publish directory ([`DiskSink`]) or memory ([`MemorySink`],
//! for `serve` and tests).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use ssg_base::Sink;
use ssg_base::paths::OutputPath;

/// Writes files below a directory, creating parent directories as needed. A file is always
/// truncated and rewritten (modes follow the process umask).
#[derive(Clone, Debug)]
pub struct DiskSink {
    pub root: PathBuf,
}

impl DiskSink {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The file system path of `path`.
    #[must_use]
    pub fn file_path(&self, path: &OutputPath) -> PathBuf {
        self.root.join(path.relative())
    }
}

impl Sink for DiskSink {
    fn write(&self, path: &OutputPath, bytes: &[u8]) -> io::Result<()> {
        let file = self.file_path(path);
        match fs::write(&file, bytes) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if let Some(parent) = file.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&file, bytes)
            }
            other => other,
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
