//! Classifying a changed path: content, static file, configuration, layout or ignored, by the mount
//! it is in.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Config,
    Static,
    Content,
    Other,
}

/// A watched mount, as far as events are concerned.
#[derive(Clone, Debug)]
pub(super) struct WatchedMount {
    pub(super) abs: PathBuf,
    pub(super) component: Component,
    pub(super) watched: bool,
}

/// Sorts file events into [`Changes`] (Go's `handleEvents` filters).
#[derive(Clone, Debug)]
pub(crate) struct Classifier {
    pub(super) mounts: Vec<WatchedMount>,
    pub(super) config: ConfigPlaces,
}

impl Classifier {
    pub(crate) fn new(vfs: &Vfs, config: ConfigPlaces) -> Self {
        Self {
            mounts: vfs
                .mounts()
                .iter()
                .filter(|m| !m.is_disabled())
                .map(|m| WatchedMount {
                    abs: m.abs.clone(),
                    component: m.component,
                    watched: !m.disable_watch,
                })
                .collect(),
            config,
        }
    }

    /// The changes of a batch of debounced events.
    pub(crate) fn classify(&self, events: &[DebouncedEvent]) -> Changes {
        let mut changes = Changes::default();
        for e in events {
            if e.need_rescan() {
                changes.rescan = true;
                continue;
            }
            let kind = &e.kind;
            let wrote = match kind {
                // The poll watcher reports a write as a newer modification time.
                EventKind::Modify(ModifyKind::Metadata(MetadataKind::WriteTime)) => true,
                // Opening, reading and closing (a write is also a modification), permission or
                // time changes (Go skips chmod events).
                EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_)) => continue,
                _ => false,
            };
            for (i, path) in e.paths.iter().enumerate() {
                let (written, must_exist) = match kind {
                    _ if wrote => (true, true),
                    EventKind::Create(_) | EventKind::Modify(ModifyKind::Data(_)) => (true, true),
                    EventKind::Modify(ModifyKind::Name(RenameMode::To)) => (true, false),
                    // The second path of a rename is where the file went.
                    EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => (i == 1, false),
                    EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) => (false, false),
                    _ => (true, false),
                };
                // A file created or written and gone again (an editor's temporary file).
                if must_exist && !path.exists() {
                    continue;
                }
                if let Some(k) = self.kind(path) {
                    changes.push(k, path.clone(), written);
                }
            }
        }
        changes
    }

    pub(super) fn kind(&self, path: &Path) -> Option<Kind> {
        if self.config.is_file(path) {
            return Some(Kind::Config);
        }
        if is_ignored(path) {
            return None;
        }
        if self.config.contains(path) {
            return Some(Kind::Config);
        }
        // The watched mounts holding the path: content wins, then anything but static.
        let mut kind: Option<Kind> = None;
        for m in self
            .mounts
            .iter()
            .filter(|m| m.watched && path.starts_with(&m.abs))
        {
            // Go skips these directories when it walks the mounts.
            let below = path.strip_prefix(&m.abs).unwrap_or(path);
            if below.components().any(|c| {
                matches!(c, PathComponent::Normal(n)
                    if n == ".git" || n == "node_modules" || n == "bower_components")
            }) {
                return None;
            }
            let k = match m.component {
                Component::Static => Kind::Static,
                Component::Content => Kind::Content,
                _ => Kind::Other,
            };
            kind = Some(match (kind, k) {
                (None, k) => k,
                (Some(Kind::Content), _) | (_, Kind::Content) => Kind::Content,
                (Some(Kind::Static), Kind::Static) => Kind::Static,
                _ => Kind::Other,
            });
        }
        kind
    }
}

/// Editors' temporary and backup files, and names Go ignores (a leading `.` or `#`, a
/// trailing `~`).
pub(super) fn is_ignored(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let ext = name.rfind('.').map_or("", |i| &name[i..]);
    name.starts_with('.')
        || name.starts_with('#')
        || name.ends_with('~')
        || name == "4913"
        || matches!(ext, ".swp" | ".swx" | ".bck" | ".tmp")
        || ext.starts_with(".goutputstream")
        || ext.starts_with(".sb-")
        || ["jb_old___", "jb_tmp___", "jb_bak___"]
            .iter()
            .any(|s| ext.ends_with(s))
}
