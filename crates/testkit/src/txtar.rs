//! Go's txtar archive format (`golang.org/x/tools/txtar`), used for small test sites such as
//! `testdata/oracle/commands/e2e/mini.txtar`.
//!
//! An archive is a comment followed by files. A file starts at a marker line `-- name --`
//! and runs to the next marker; a comment or file that is not empty always ends with a newline.

use std::fs;
use std::io;
use std::path::{Component, Path};

/// A parsed txtar archive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Archive {
    pub comment: String,
    pub files: Vec<File>,
}

/// One file of an [`Archive`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// The name from the marker line, trimmed; a relative, slash-separated path for a site.
    pub name: String,
    pub data: String,
}

/// The name of a marker line (`-- name --`), without its line break.
fn marker_name(line: &str) -> Option<&str> {
    let inner = line.strip_prefix("-- ")?.strip_suffix(" --")?;
    Some(inner.trim())
}

fn fix_nl(mut s: String) -> String {
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

impl Archive {
    /// Parses an archive. Every input is a valid archive; text before the first marker is the
    /// comment.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut archive = Self::default();
        let mut current: Option<(String, String)> = None;
        let mut rest = text;
        while !rest.is_empty() {
            let (line, next) = match rest.find('\n') {
                Some(i) => (&rest[..i], &rest[i + 1..]),
                None => (rest, ""),
            };
            let full = &rest[..rest.len() - next.len()];
            if let Some(name) = marker_name(line) {
                match current.take() {
                    Some((n, d)) => archive.files.push(File {
                        name: n,
                        data: fix_nl(d),
                    }),
                    None => archive.comment = fix_nl(std::mem::take(&mut archive.comment)),
                }
                current = Some((name.to_owned(), String::new()));
            } else {
                match &mut current {
                    Some((_, data)) => data.push_str(full),
                    None => archive.comment.push_str(full),
                }
            }
            rest = next;
        }
        match current {
            Some((n, d)) => archive.files.push(File {
                name: n,
                data: fix_nl(d),
            }),
            None => archive.comment = fix_nl(archive.comment),
        }
        archive
    }

    /// Reads and parses an archive file.
    ///
    /// # Errors
    /// When the file cannot be read or is not UTF-8.
    pub fn read(path: &Path) -> io::Result<Self> {
        Ok(Self::parse(&fs::read_to_string(path)?))
    }

    /// The data of the last file named `name` (a later file of the same name wins, as when the
    /// archive is written out).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.files
            .iter()
            .rev()
            .find(|f| f.name == name)
            .map(|f| f.data.as_str())
    }

    /// Writes every file under `dir`, creating directories as needed.
    ///
    /// # Errors
    /// I/O errors, and `InvalidInput` for a name that is empty, absolute or leaves `dir`.
    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        for f in &self.files {
            let rel = Path::new(&f.name);
            let inside =
                !f.name.is_empty() && rel.components().all(|c| matches!(c, Component::Normal(_)));
            if !inside {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "txtar file name {:?} is not a relative path inside the site",
                        f.name
                    ),
                ));
            }
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, &f.data)?;
        }
        Ok(())
    }
}
