//! The assets union view as the pipes see it: files and directories by asset path, and the
//! asset path of a real file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ssg_jsbuild::{AssetEntry, Assets};
use ssg_vfs::{Component, Vfs};

/// The assets component of a store's [`Vfs`] (empty without one).
#[derive(Clone, Copy)]
pub(super) struct AssetsView<'a> {
    vfs: Option<&'a Vfs>,
}

impl<'a> AssetsView<'a> {
    pub(super) fn new(vfs: Option<&'a Vfs>) -> Self {
        Self { vfs }
    }

    /// The real file at asset path `rel` (`/`-separated, a leading `/` ignored).
    pub(super) fn file(&self, rel: &str) -> Option<PathBuf> {
        let rel = clean(rel);
        if rel.is_empty() {
            return None;
        }
        self.vfs?.open(Component::Assets, &rel).map(|f| f.abs)
    }

    /// Whether asset path `rel` is a directory in one of the mounts.
    pub(super) fn is_dir(&self, rel: &str) -> bool {
        let rel = clean(rel);
        let Some(vfs) = self.vfs else {
            return false;
        };
        vfs.mounts_of(Component::Assets).any(|(_, m)| {
            let target = m.target_dir();
            let rest = if target.is_empty() {
                Some(rel.as_str())
            } else if rel == target {
                Some("")
            } else {
                rel.strip_prefix(target).and_then(|r| r.strip_prefix('/'))
            };
            rest.is_some_and(|r| {
                !r.split('/').any(|s| s == "..")
                    && (if r.is_empty() {
                        m.abs.clone()
                    } else {
                        m.abs.join(r)
                    })
                    .is_dir()
            })
        })
    }

    /// The asset path of a real file or directory, when it lies under an assets mount.
    pub(super) fn asset_path(&self, filename: &Path) -> Option<String> {
        self.vfs?.mounts_of(Component::Assets).find_map(|(_, m)| {
            let rel = filename.strip_prefix(&m.abs).ok()?;
            let rel = rel.to_str()?.replace('\\', "/");
            let target = m.target_dir();
            Some(match (target.is_empty(), rel.is_empty()) {
                (true, _) => rel,
                (false, true) => target.to_owned(),
                (false, false) => format!("{target}/{rel}"),
            })
        })
    }
}

impl AssetsView<'_> {
    fn entry(&self, path: &str) -> Option<AssetEntry> {
        if let Some(f) = self.file(path) {
            return Some(AssetEntry::File(f));
        }
        self.is_dir(path).then_some(AssetEntry::Dir)
    }
}

/// The [`AssetsView`] of a shared [`Vfs`]: `js_build` resolves imports on the bundler's
/// threads, so it needs a view it can keep.
pub(super) struct SharedAssets(pub(super) Option<Arc<Vfs>>);

impl Assets for SharedAssets {
    fn entry(&self, path: &str) -> Option<AssetEntry> {
        AssetsView::new(self.0.as_deref()).entry(path)
    }

    fn assets_path(&self, filename: &Path) -> Option<String> {
        AssetsView::new(self.0.as_deref()).asset_path(filename)
    }
}

/// `rel` cleaned, without leading or trailing `/` (`""` for the root).
pub(super) fn clean(rel: &str) -> String {
    let c = ssg_base::paths::clean(&format!("/{}", rel.replace('\\', "/")));
    c.trim_matches('/').to_owned()
}
