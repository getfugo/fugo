//! What the server serves: the files of the last good build (memory, or the publish directory
//! with `--render-to-disk`), where each listener's site starts, and the media types of the
//! files.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use ssg_base::Sink;
use ssg_base::paths::OutputPath;
use ssg_config::Config;
use ssg_publish::{DiskSink, MemorySink};

/// The files being served.
#[derive(Clone, Debug)]
pub(crate) enum Tree {
    /// A memory build's sink: replaced as a whole after each successful build; static-only
    /// changes are written into it.
    Memory(Arc<MemorySink>),
    /// The publish directory (`--render-to-disk`).
    Disk(PathBuf),
}

impl Tree {
    /// The file at `rel` (a clean path without leading slash).
    pub(crate) async fn read(&self, rel: &str) -> Option<Bytes> {
        match self {
            Self::Memory(m) => m.get(rel).map(Bytes::from_owner),
            Self::Disk(root) => {
                let path = root.join(rel);
                match tokio::fs::metadata(&path).await {
                    Ok(meta) if meta.is_file() => {
                        tokio::fs::read(&path).await.ok().map(Bytes::from)
                    }
                    _ => None,
                }
            }
        }
    }

    /// Whether `rel` is a file.
    pub(crate) async fn is_file(&self, rel: &str) -> bool {
        match self {
            Self::Memory(m) => m.files.contains_key(&OutputPath::new(rel)),
            Self::Disk(root) => tokio::fs::metadata(root.join(rel))
                .await
                .is_ok_and(|m| m.is_file()),
        }
    }

    /// The file at `rel` now (blocking; the watch thread's).
    pub(crate) fn read_now(&self, rel: &str) -> Option<Vec<u8>> {
        match self {
            Self::Memory(m) => m.get(rel).map(|b| b.to_vec()),
            Self::Disk(root) => fs::read(root.join(OutputPath::new(rel).relative())).ok(),
        }
    }

    /// Writes a static file.
    pub(crate) fn write(&self, rel: &str, bytes: &[u8]) -> io::Result<()> {
        let path = OutputPath::new(rel);
        match self {
            Self::Memory(m) => m.write(&path, bytes),
            Self::Disk(root) => DiskSink::new(root.clone()).write(&path, bytes),
        }
    }

    /// Removes a static file that is gone from every static directory.
    pub(crate) fn remove(&self, rel: &str) -> io::Result<()> {
        match self {
            Self::Memory(m) => {
                m.files.remove(&OutputPath::new(rel));
                Ok(())
            }
            Self::Disk(root) => match fs::remove_file(root.join(OutputPath::new(rel).relative())) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            },
        }
    }
}

/// One listener's site: all languages, or one language of a multihost site.
#[derive(Clone, Debug)]
pub(crate) struct Host {
    /// The base URL's path (`/`, `/docs/`), decoded: requests outside it are not the site's.
    pub(crate) base_path: String,
    /// The site's directory in the tree (`""`, or `fr/` on a multihost site).
    pub(crate) root: String,
    /// The language directories below the root (`nn`), longest first: a miss below one gets
    /// that language's 404 page.
    pub(crate) languages: Vec<String>,
}

/// The served state: swapped as a whole after each successful build.
#[derive(Debug)]
pub(crate) struct Served {
    pub(crate) tree: Tree,
    /// Indexed like the listeners.
    pub(crate) hosts: Vec<Host>,
    pub(crate) media_types: MediaTypes,
}

impl Served {
    /// Nothing, until the first build.
    pub(crate) fn empty() -> Self {
        Self {
            tree: Tree::Memory(Arc::default()),
            hosts: Vec::new(),
            media_types: MediaTypes::default(),
        }
    }

    pub(crate) fn new(tree: Tree, cfg: &Config) -> Self {
        let languages = || {
            let mut dirs: Vec<String> = cfg
                .sites
                .iter()
                .map(|s| s.language.url_prefix.clone())
                .filter(|p| !p.is_empty())
                .collect();
            dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
            dirs
        };
        let hosts = if cfg.multihost {
            cfg.sites
                .iter()
                .map(|s| Host {
                    base_path: s.base_url.base_path().to_owned(),
                    root: format!("{}/", s.language.key),
                    languages: Vec::new(),
                })
                .collect()
        } else {
            vec![Host {
                base_path: cfg.default_site().base_url.base_path().to_owned(),
                root: String::new(),
                languages: languages(),
            }]
        };
        Self {
            tree,
            hosts,
            media_types: MediaTypes::from_config(cfg),
        }
    }
}

/// `Content-Type` by file suffix: the site's media types (a later type in type order wins a suffix,
/// `text/*` with `charset=utf-8`), as the Go implementation registers them with Go's `mime`
/// package, then the common types of `mime_guess`.
#[derive(Debug, Default)]
pub(crate) struct MediaTypes(HashMap<String, String>);

impl MediaTypes {
    pub(crate) fn from_config(cfg: &Config) -> Self {
        let mut map = HashMap::new();
        for (_, t) in cfg.media_types.iter() {
            for s in &t.suffixes {
                map.insert(s.to_ascii_lowercase(), with_charset(t.type_string()));
            }
        }
        Self(map)
    }

    /// The media type of the file at `rel`; a suffix nobody knows is sniffed as text or
    /// bytes.
    pub(crate) fn of(&self, rel: &str, bytes: &[u8]) -> String {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let suffix = name
            .rsplit_once('.')
            .map(|(_, s)| s.to_ascii_lowercase())
            .unwrap_or_default();
        if let Some(t) = self.0.get(&suffix) {
            return t.clone();
        }
        if let Some(t) = mime_guess::from_ext(&suffix).first_raw() {
            return with_charset(t.to_owned());
        }
        if std::str::from_utf8(bytes).is_ok_and(|s| !s.contains('\0')) {
            "text/plain; charset=utf-8".to_owned()
        } else {
            "application/octet-stream".to_owned()
        }
    }
}

fn with_charset(t: String) -> String {
    if t.starts_with("text/") && !t.contains("charset") {
        format!("{t}; charset=utf-8")
    } else {
        t
    }
}

/// The publish directory of a configuration (absolute).
pub(crate) fn publish_dir(cfg: &Config) -> PathBuf {
    if cfg.dirs.publish.is_absolute() {
        cfg.dirs.publish.clone()
    } else {
        cfg.project_dir.join(&cfg.dirs.publish)
    }
}
