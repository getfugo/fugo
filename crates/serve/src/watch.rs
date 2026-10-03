//! Watching the project: what is watched (Go's watch set), the watcher itself (notify with a
//! 1 s debounce, or polling), and what a batch of events means for the site.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component as PathComponent, Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use notify::event::{EventKind, MetadataKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventHandler, PollWatcher, RecommendedWatcher, RecursiveMode, WatcherKind};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, NoCache, RecommendedCache, new_debouncer_opt,
};
use ssg_config::Config;
use ssg_vfs::{Component, Vfs};

use crate::ServeError;

/// How long events are held back so that one save (or a burst of saves) is one rebuild.
pub(crate) const DEBOUNCE: Duration = Duration::from_secs(1);

/// How often the debouncer looks for events whose time is up.
const TICK: Duration = Duration::from_millis(100);

/// What the watch loop receives.
pub(crate) enum Message {
    Events(DebounceEventResult),
    Shutdown,
}

/// Where configuration lives: the files it was read from (the project's and its themes', the
/// project's `.env` files, and its `package.json`, whose packages are installed when the
/// configuration loads), the configuration directories (the project's `--configDir` and each
/// theme's `config/`), and the directories whose `config.*` file is configuration even when it
/// did not exist yet (the project's and each theme's).
#[derive(Clone, Debug)]
pub(crate) struct ConfigPlaces {
    files: BTreeSet<PathBuf>,
    dirs: Vec<PathBuf>,
    homes: Vec<PathBuf>,
    names: BTreeSet<String>,
}

impl ConfigPlaces {
    pub(crate) fn new(cfg: &Config, config_dir: &Path) -> Self {
        let mut dirs = vec![config_dir.to_path_buf()];
        let mut homes = vec![cfg.project_dir.clone()];
        for t in &cfg.themes {
            dirs.push(t.dir.join("config"));
            homes.push(t.dir.clone());
        }
        let mut files: BTreeSet<PathBuf> = cfg.config_files.iter().cloned().collect();
        // The `.env` files `get_env` reads, whether or not they exist yet.
        for name in ssg_config::env_file::file_names(&cfg.environment) {
            files.insert(cfg.project_dir.join(name));
        }
        files.insert(cfg.project_dir.join("package.json"));
        Self {
            files,
            dirs,
            homes,
            names: ssg_config::config_file_names().collect(),
        }
    }

    /// Whether `path` is one of the configuration files (named by its path, so a leading `.`
    /// does not make it an editor's file: the project's `.env`).
    fn is_file(&self, path: &Path) -> bool {
        self.files.contains(path)
    }

    /// Whether a change of `path` changes the configuration.
    fn contains(&self, path: &Path) -> bool {
        self.files.contains(path)
            || self.dirs.iter().any(|d| path.starts_with(d))
            || (path
                .parent()
                .is_some_and(|p| self.homes.iter().any(|h| h == p))
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| self.names.contains(n)))
    }
}

/// The directories to watch, each with its mode, and what they are watched for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WatchSet {
    pub(crate) roots: BTreeMap<PathBuf, RecursiveMode>,
    /// The mounted directories and files (for the report).
    pub(crate) sources: Vec<PathBuf>,
    /// The configuration files and directories (for the report).
    pub(crate) config: Vec<PathBuf>,
}

impl WatchSet {
    /// Every mount of the project and its themes that is not `disableWatch` (a directory with
    /// everything below it; a mounted file through its directory), the configuration
    /// directories (with everything below them), and the directories of the configuration
    /// files, of the project and of each theme (only their own files). Missing paths are left
    /// out.
    pub(crate) fn new(vfs: &Vfs, config: &ConfigPlaces) -> Self {
        let mut recursive: BTreeSet<PathBuf> = BTreeSet::new();
        let mut flat: BTreeSet<PathBuf> = BTreeSet::new();
        let mut sources: BTreeSet<PathBuf> = BTreeSet::new();
        for m in vfs.mounts() {
            if m.disable_watch || m.is_disabled() {
                continue;
            }
            if m.abs.is_dir() {
                recursive.insert(m.abs.clone());
                sources.insert(m.abs.clone());
            } else if m.abs.is_file()
                && let Some(parent) = m.abs.parent()
            {
                flat.insert(parent.to_path_buf());
                sources.insert(m.abs.clone());
            }
        }
        let mut report: Vec<PathBuf> = config.files.iter().cloned().collect();
        for d in &config.dirs {
            if d.is_dir() {
                recursive.insert(d.clone());
                report.push(d.clone());
            }
        }
        let parents = config.files.iter().filter_map(|f| f.parent());
        for dir in parents.chain(config.homes.iter().map(PathBuf::as_path)) {
            if dir.is_dir() {
                flat.insert(dir.to_path_buf());
            }
        }
        // A root inside a recursive root adds nothing.
        let mut roots = BTreeMap::new();
        let covered = |p: &Path, roots: &BTreeMap<PathBuf, RecursiveMode>| {
            roots
                .iter()
                .any(|(r, m)| *m == RecursiveMode::Recursive && p.starts_with(r))
        };
        for p in recursive {
            if !covered(&p, &roots) {
                roots.insert(p, RecursiveMode::Recursive);
            }
        }
        for p in flat {
            if !covered(&p, &roots) {
                roots.insert(p, RecursiveMode::NonRecursive);
            }
        }
        Self {
            roots,
            sources: sources.into_iter().collect(),
            config: report,
        }
    }
}

/// The file watcher: the platform's notifications or polling, debounced.
pub(crate) enum Watcher {
    Native(Debouncer<NativeWatcher, RecommendedCache>),
    Poll(Debouncer<PollWatcher, NoCache>),
}

/// The paths a watched path resolves to, and the watched path: (resolved, watched).
type Aliases = Arc<RwLock<Vec<(PathBuf, PathBuf)>>>;

/// The platform's watcher, its events put in terms of what was watched before the debouncer
/// sees them.
///
/// - macOS (FSEvents) reports events under a watched directory's resolved path (`/private/var/…`
///   for `/var/…`, a symlink's target), which no mount or configuration path starts with: they
///   are mapped back to the watched path.
/// - FSEvents repeats a file's earlier changes with a later one: a removal arrives as "created,
///   removed, modified". The debouncer drops the creation together with the removal (a file that
///   came and went), and the modification of a file that is gone then looks like an editor's
///   temporary file, so the removal was lost. On macOS an event (but a rename) for a path that
///   no longer exists is a removal.
pub(crate) struct NativeWatcher {
    inner: RecommendedWatcher,
    aliases: Aliases,
}

impl notify::Watcher for NativeWatcher {
    fn new<F: EventHandler>(mut handler: F, config: notify::Config) -> notify::Result<Self> {
        let aliases = Aliases::default();
        let map = Arc::clone(&aliases);
        let inner = <RecommendedWatcher as notify::Watcher>::new(
            move |r: notify::Result<Event>| handler.handle_event(r.map(|e| as_watched(e, &map))),
            config,
        )?;
        Ok(Self { inner, aliases })
    }

    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        notify::Watcher::watch(&mut self.inner, path, mode)?;
        if let Ok(resolved) = path.canonicalize()
            && resolved != path
        {
            write(&self.aliases).push((resolved, path.to_path_buf()));
        }
        Ok(())
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        write(&self.aliases).retain(|(_, watched)| watched != path);
        notify::Watcher::unwatch(&mut self.inner, path)
    }

    fn configure(&mut self, option: notify::Config) -> notify::Result<bool> {
        notify::Watcher::configure(&mut self.inner, option)
    }

    fn kind() -> WatcherKind {
        <RecommendedWatcher as notify::Watcher>::kind()
    }
}

/// `e` with its paths under the watched paths they resolve from, and on macOS an event on a
/// path that is gone made a removal (see [`NativeWatcher`]).
fn as_watched(mut e: Event, aliases: &RwLock<Vec<(PathBuf, PathBuf)>>) -> Event {
    let aliases = aliases.read().unwrap_or_else(PoisonError::into_inner);
    for path in &mut e.paths {
        // The most specific watched path the event's path resolves under.
        let alias = aliases
            .iter()
            .filter(|(resolved, _)| path.starts_with(resolved))
            .max_by_key(|(resolved, _)| resolved.as_os_str().len());
        if let Some((resolved, watched)) = alias
            && let Ok(rest) = path.strip_prefix(resolved)
        {
            *path = watched.join(rest);
        }
    }
    if cfg!(target_os = "macos") && gone(&e) {
        e.kind = EventKind::Remove(RemoveKind::Any);
    }
    e
}

/// A creation or modification of one path that does not exist (renames are the debouncer's).
fn gone(e: &Event) -> bool {
    matches!(
        e.kind,
        EventKind::Create(_)
            | EventKind::Modify(
                ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Metadata(_) | ModifyKind::Other
            )
    ) && matches!(e.paths.as_slice(), [path] if !path.exists())
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

impl Watcher {
    /// A watcher that sends its batches to `tx`.
    pub(crate) fn new(poll: Option<Duration>, tx: Sender<Message>) -> Result<Self, ServeError> {
        let handler = move |r: DebounceEventResult| {
            let _ = tx.send(Message::Events(r));
        };
        let watcher = match poll {
            None => Self::Native(new_debouncer_opt(
                DEBOUNCE,
                Some(TICK),
                handler,
                RecommendedCache::new(),
                notify::Config::default(),
            )?),
            // notify's poller keeps modification times in whole seconds, so it also compares
            // contents: a second save within the same second is still seen.
            Some(interval) => Self::Poll(new_debouncer_opt(
                DEBOUNCE,
                Some(TICK),
                handler,
                NoCache,
                notify::Config::default()
                    .with_poll_interval(interval)
                    .with_compare_contents(true),
            )?),
        };
        Ok(watcher)
    }

    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        match self {
            Self::Native(d) => d.watch(path, mode),
            Self::Poll(d) => d.watch(path, mode),
        }
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        match self {
            Self::Native(d) => d.unwatch(path),
            Self::Poll(d) => d.unwatch(path),
        }
    }

    /// Moves from watching `old` to watching `new`. A path that cannot be watched (removed
    /// meanwhile) is skipped and returned.
    pub(crate) fn update(
        &mut self,
        old: &WatchSet,
        new: &WatchSet,
    ) -> Vec<(PathBuf, notify::Error)> {
        let mut failed = Vec::new();
        for (p, mode) in &old.roots {
            if new.roots.get(p) != Some(mode) {
                let _ = self.unwatch(p);
            }
        }
        for (p, mode) in &new.roots {
            if old.roots.get(p) != Some(mode)
                && let Err(e) = self.watch(p, *mode)
            {
                failed.push((p.clone(), e));
            }
        }
        failed
    }
}

/// What a batch of file events changed.
#[derive(Debug, Default)]
pub(crate) struct Changes {
    /// Configuration files (a configuration change rebuilds everything).
    pub(crate) config: Vec<PathBuf>,
    /// Static files and directories.
    pub(crate) statics: Vec<PathBuf>,
    /// Content files, with whether the event wrote or created them.
    pub(crate) content: Vec<(PathBuf, bool)>,
    /// Everything else of the site: layouts, assets, data, i18n, archetypes.
    pub(crate) other: Vec<PathBuf>,
    /// The watcher lost events: rebuild everything.
    pub(crate) rescan: bool,
}

impl Changes {
    pub(crate) fn is_empty(&self) -> bool {
        self.config.is_empty()
            && self.statics.is_empty()
            && self.content.is_empty()
            && self.other.is_empty()
            && !self.rescan
    }

    /// Whether only static files changed.
    pub(crate) fn static_only(&self) -> bool {
        !self.statics.is_empty()
            && self.config.is_empty()
            && self.content.is_empty()
            && self.other.is_empty()
            && !self.rescan
    }

    /// The changed paths, for the report.
    pub(crate) fn paths(&self) -> Vec<PathBuf> {
        let mut all: BTreeSet<PathBuf> = BTreeSet::new();
        all.extend(self.config.iter().cloned());
        all.extend(self.statics.iter().cloned());
        all.extend(self.content.iter().map(|(p, _)| p.clone()));
        all.extend(self.other.iter().cloned());
        all.into_iter().collect()
    }

    fn push(&mut self, kind: Kind, path: PathBuf, written: bool) {
        let list = match kind {
            Kind::Config => &mut self.config,
            Kind::Static => &mut self.statics,
            Kind::Content => {
                match self.content.iter_mut().find(|(p, _)| *p == path) {
                    Some(seen) => seen.1 |= written,
                    None => self.content.push((path, written)),
                }
                return;
            }
            Kind::Other => &mut self.other,
        };
        if !list.contains(&path) {
            list.push(path);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Config,
    Static,
    Content,
    Other,
}

/// A watched mount, as far as events are concerned.
#[derive(Clone, Debug)]
struct WatchedMount {
    abs: PathBuf,
    component: Component,
    watched: bool,
}

/// Sorts file events into [`Changes`] (Go's `handleEvents` filters).
#[derive(Clone, Debug)]
pub(crate) struct Classifier {
    mounts: Vec<WatchedMount>,
    config: ConfigPlaces,
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

    fn kind(&self, path: &Path) -> Option<Kind> {
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
fn is_ignored(path: &Path) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_mapped_to_the_watched_paths() {
        let aliases = RwLock::new(vec![
            (PathBuf::from("/private/var/t"), PathBuf::from("/var/t")),
            (
                PathBuf::from("/private/var/t/site/static"),
                PathBuf::from("/var/t/site/static"),
            ),
            (
                PathBuf::from("/real/theme"),
                PathBuf::from("/var/t/themes/x"),
            ),
        ]);
        let event = |path: &str| {
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Any))).add_path(path.into())
        };
        let mapped = |path: &str| as_watched(event(path), &aliases).paths;
        assert_eq!(
            mapped("/private/var/t/site/content/a.md"),
            [PathBuf::from("/var/t/site/content/a.md")]
        );
        assert_eq!(
            mapped("/private/var/t/site/static/b.css"),
            [PathBuf::from("/var/t/site/static/b.css")]
        );
        assert_eq!(
            mapped("/real/theme/layouts/home.html"),
            [PathBuf::from("/var/t/themes/x/layouts/home.html")]
        );
        // Paths no alias holds stay as they are (and `/private/var/tt` is not under `/private/var/t`).
        assert_eq!(mapped("/elsewhere/x"), [PathBuf::from("/elsewhere/x")]);
        assert_eq!(
            mapped("/private/var/tt/x"),
            [PathBuf::from("/private/var/tt/x")]
        );
    }

    #[test]
    fn events_on_gone_paths_are_removals_on_macos() {
        let aliases = RwLock::new(Vec::new());
        let gone_path = std::env::temp_dir().join(format!("ssg-gone-{}", std::process::id()));
        for kind in [
            EventKind::Create(notify::event::CreateKind::File),
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
        ] {
            let e = as_watched(Event::new(kind).add_path(gone_path.clone()), &aliases);
            let want = if cfg!(target_os = "macos") {
                EventKind::Remove(RemoveKind::Any)
            } else {
                kind
            };
            assert_eq!(e.kind, want);
        }
        // An existing path keeps its event, and a rename is left to the debouncer.
        let here = std::env::temp_dir();
        let created = EventKind::Create(notify::event::CreateKind::Folder);
        assert_eq!(
            as_watched(Event::new(created).add_path(here), &aliases).kind,
            created
        );
        let renamed = EventKind::Modify(ModifyKind::Name(RenameMode::From));
        assert_eq!(
            as_watched(Event::new(renamed).add_path(gone_path), &aliases).kind,
            renamed
        );
    }

    #[test]
    fn ignored_names() {
        for name in [
            ".config.toml.swp",
            "one.md~",
            "4913",
            "#one.md#",
            ".#one.md",
            "one.md.tmp",
            "one.md.swx",
            "a.css___jb_tmp___",
            "x.goutputstream-ABC",
            ".DS_Store",
            "one.md.sb-123",
        ] {
            assert!(is_ignored(Path::new(name)), "{name}");
        }
        for name in ["one.md", "site.css", "index.html", "4914", "a~b.md"] {
            assert!(!is_ignored(Path::new(name)), "{name}");
        }
    }
}
