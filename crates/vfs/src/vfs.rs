//! The union file view of each component.

use std::borrow::Cow;
use std::cmp::Reverse;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ssg_base::{Idx, LangIdx};
use ssg_config::Config;

use crate::filter::IgnoreRules;
use crate::mount::{self, Module, Mount};
use crate::{Component, VfsError};

/// A file of a component's union view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRef {
    pub component: Component,
    /// The path inside the component: `/`-separated, original case, no leading slash
    /// (`posts/My Post.md`, `vendor/jquery/dist/jquery.js`).
    pub rel: String,
    pub abs: PathBuf,
    /// The language of the mount (`None`: the default language).
    pub mount_lang: Option<LangIdx>,
    /// The index of the mount in [`Vfs::mounts`].
    pub mount_idx: u16,
}

/// Which files of different mounts with the same path a component keeps. "First" is by
/// [`Vfs::walk`]'s order of the mounts holding a path (see [`Vfs::rank`]).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Union {
    /// The first mount's file: layouts, assets, archetypes, static.
    FirstWins,
    /// The first mount's file per mount language: content (and static of multihost sites).
    PerLanguage,
    /// Every file: data and i18n merge their files themselves.
    KeepAll,
}

/// The mounts of a project and the union file view of each component: for every component the
/// project's mounts come first, then the themes', and the first mount holding a path wins
/// (static: the last mount of the first module holding it, see [`Vfs::walk`]).
#[derive(Clone, Debug)]
pub struct Vfs {
    mounts: Vec<Mount>,
    ignore: IgnoreRules,
    multihost: bool,
    /// The number of languages (each one's static files are copied apart when multihost).
    languages: usize,
}

impl Vfs {
    /// The mounts of `cfg`: `[[module.mounts]]` (a missing source is skipped), then the default
    /// mount of each component no mount targets (per language for content, from
    /// `languages.<lang>.contentDir`; per static dir for static), the root JS config files
    /// (`assets/_jsconfig`), and the mounts of each theme of `Config::themes` (its configured
    /// mounts, else its component directories, and its JS config files).
    ///
    /// # Errors
    /// A mount with an unknown language or an invalid glob, an invalid `ignoreFiles` pattern,
    /// or a missing theme.
    pub fn new(cfg: &Config) -> Result<Self, VfsError> {
        let ignore = IgnoreRules::new(&cfg.ignore_files)
            .map_err(|(pattern, error)| VfsError::IgnorePattern { pattern, error })?;
        Ok(Self {
            mounts: mount::mounts(cfg)?,
            ignore,
            multihost: cfg.multihost,
            languages: cfg.sites.len(),
        })
    }

    /// Every mount, in precedence order.
    #[must_use]
    pub fn mounts(&self) -> &[Mount] {
        &self.mounts
    }

    /// The mounts of component `c` with their indices, in precedence order.
    pub fn mounts_of(&self, c: Component) -> impl Iterator<Item = (u16, &Mount)> + '_ {
        self.mounts
            .iter()
            .enumerate()
            .filter(move |(_, m)| m.component == c && !m.is_disabled())
            .map(|(i, m)| (u16::try_from(i).unwrap_or(u16::MAX), m))
    }

    fn union(&self, c: Component) -> Union {
        match c {
            Component::Content => Union::PerLanguage,
            Component::Static if self.multihost => Union::PerLanguage,
            Component::Data | Component::I18n => Union::KeepAll,
            _ => Union::FirstWins,
        }
    }

    /// Every file of component `c`, sorted by path (byte order) and, for one path, by mount
    /// precedence. The ignore rules and the mounts' file filters are applied. Files hidden by
    /// an earlier mount are left out, except for data and i18n, which keep every file.
    ///
    /// Static follows Go's static copy: of the mounts of one module holding a path the
    /// *last* wins (Go's root-mapping file system opens the last file mount, and its sync
    /// copies the mounts one after the other), and the project still wins over the themes.
    /// Symbolic links below a static mount root are followed (a dangling link is skipped, and
    /// so is a link to a directory it is inside of); for every other component they are
    /// skipped. On a multihost site every static file has a language: the file of a mount
    /// without one is listed once per language, as Go copies such a mount to every
    /// language's directory.
    ///
    /// # Errors
    /// A directory that cannot be read, or a file name that is not UTF-8.
    pub fn walk(&self, c: Component) -> Result<Vec<FileRef>, VfsError> {
        let mut files = Vec::new();
        for (idx, m) in self.mounts_of(c) {
            let meta = match fs::metadata(&m.abs) {
                Ok(meta) => meta,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(VfsError::io(&m.abs, e)),
            };
            if meta.is_file() {
                let rel = file_mount_rel(m);
                let name = rel.rsplit('/').next().unwrap_or_default();
                if !self.ignore.skips(c, name, &m.abs, false) {
                    files.push(file_ref(m, idx, rel, m.abs.clone()));
                }
            } else {
                let mut stack = Vec::new();
                if c == Component::Static {
                    stack.push(fs::canonicalize(&m.abs).map_err(|e| VfsError::io(&m.abs, e))?);
                }
                self.walk_dir(m, idx, &m.abs, "", &mut stack, &mut files)?;
            }
        }
        if c == Component::Static && self.multihost {
            files = files
                .into_iter()
                .flat_map(|f| {
                    let langs: Vec<LangIdx> = match f.mount_lang {
                        Some(l) => vec![l],
                        None => (0..self.languages).map(LangIdx::from_index).collect(),
                    };
                    langs.into_iter().map(move |l| FileRef {
                        mount_lang: Some(l),
                        ..f.clone()
                    })
                })
                .collect();
        }
        // A stable sort: for one path the files are in mount order, or `rank` order for static.
        if c == Component::Static {
            files.sort_by(|a, b| {
                (&a.rel, self.rank(a.mount_idx)).cmp(&(&b.rel, self.rank(b.mount_idx)))
            });
        } else {
            files.sort_by(|a, b| a.rel.cmp(&b.rel));
        }

        let union = self.union(c);
        if union != Union::KeepAll {
            let default = LangIdx::from_index(0);
            let mut kept: Vec<FileRef> = Vec::with_capacity(files.len());
            let mut group_start = 0;
            for f in files {
                if kept.get(group_start).is_none_or(|g| g.rel != f.rel) {
                    group_start = kept.len();
                }
                let hidden = kept[group_start..].iter().any(|k| {
                    union == Union::FirstWins
                        || k.mount_lang.unwrap_or(default) == f.mount_lang.unwrap_or(default)
                });
                if !hidden {
                    kept.push(f);
                }
            }
            files = kept;
        }
        Ok(files)
    }

    /// The precedence of static mount `idx` among the mounts holding a path (lower wins): its
    /// module, then the later mount first.
    fn rank(&self, idx: u16) -> (Module, Reverse<u16>) {
        let module = self
            .mounts
            .get(usize::from(idx))
            .map_or(Module::Project, |m| m.module);
        (module, Reverse(idx))
    }

    /// Walks `dir`, the directory at `below` (`""` or `/a/b`) under mount `m`'s source.
    /// `stack` holds the canonical paths of `dir` and its ancestors when symbolic links are
    /// followed (static), and is empty otherwise.
    fn walk_dir(
        &self,
        m: &Mount,
        idx: u16,
        dir: &Path,
        below: &str,
        stack: &mut Vec<PathBuf>,
        out: &mut Vec<FileRef>,
    ) -> Result<(), VfsError> {
        let follow = !stack.is_empty();
        let mut entries: Vec<(String, OsString, fs::FileType)> = Vec::new();
        for e in fs::read_dir(dir).map_err(|e| VfsError::io(dir, e))? {
            let e = e.map_err(|e| VfsError::io(dir, e))?;
            let ft = e.file_type().map_err(|err| VfsError::io(e.path(), err))?;
            let os_name = e.file_name();
            let name = os_name
                .to_str()
                .map(|n| entry_name(n, NFC_NAMES).into_owned())
                .ok_or_else(|| VfsError::NonUtf8 { path: e.path() })?;
            entries.push((name, os_name, ft));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, os_name, ft) in entries {
            let abs = dir.join(&os_name);
            let (is_dir, is_file) = if !ft.is_symlink() {
                (ft.is_dir(), ft.is_file())
            } else if follow {
                match fs::metadata(&abs) {
                    Ok(meta) => (meta.is_dir(), meta.is_file()),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(VfsError::io(&abs, e)),
                }
            } else {
                continue;
            };
            let below = format!("{below}/{name}");
            if !self.admits(m, &name, &below, &abs, is_dir) {
                continue;
            }
            if is_dir {
                if follow {
                    let real = fs::canonicalize(&abs).map_err(|e| VfsError::io(&abs, e))?;
                    if stack.contains(&real) {
                        continue;
                    }
                    stack.push(real);
                    self.walk_dir(m, idx, &abs, &below, stack, out)?;
                    stack.pop();
                } else {
                    self.walk_dir(m, idx, &abs, &below, stack, out)?;
                }
            } else if is_file {
                let rel = join_rel(&m.sub, &below[1..]);
                out.push(file_ref(m, idx, rel, abs));
            }
        }
        Ok(())
    }

    fn admits(&self, m: &Mount, name: &str, below: &str, abs: &Path, is_dir: bool) -> bool {
        !self.ignore.skips(m.component, name, abs, is_dir)
            && m.filter.as_ref().is_none_or(|f| f.admits(below, is_dir))
    }

    /// The file at `rel` (a path inside component `c`) in the first mount that has it, with the
    /// same rules as [`Vfs::walk`] (static: the last mount of the first module, following
    /// symbolic links).
    #[must_use]
    pub fn open(&self, c: Component, rel: &str) -> Option<FileRef> {
        let rel = rel.trim_start_matches('/');
        let mut mounts: Vec<(u16, &Mount)> = self.mounts_of(c).collect();
        let follow = c == Component::Static;
        if follow {
            mounts.sort_by_key(|(idx, _)| self.rank(*idx));
        }
        mounts.into_iter().find_map(|(idx, m)| {
            let meta = fs::metadata(&m.abs).ok()?;
            if meta.is_file() {
                let own = file_mount_rel(m);
                return (own == rel).then(|| file_ref(m, idx, own, m.abs.clone()));
            }
            let rest = if m.sub.is_empty() {
                rel
            } else {
                rel.strip_prefix(m.sub.as_str())?.strip_prefix('/')?
            };
            let mut abs = m.abs.clone();
            let mut below = String::new();
            let segments: Vec<&str> = rest.split('/').collect();
            for (i, seg) in segments.iter().enumerate() {
                if seg.is_empty() || *seg == "." || *seg == ".." {
                    return None;
                }
                let is_dir = i + 1 < segments.len();
                abs.push(seg);
                below.push('/');
                below.push_str(seg);
                let ft = if follow {
                    fs::metadata(&abs)
                } else {
                    fs::symlink_metadata(&abs)
                }
                .ok()?
                .file_type();
                if ft.is_symlink()
                    || ft.is_dir() != is_dir
                    || !self.admits(m, seg, &below, &abs, is_dir)
                {
                    return None;
                }
            }
            Some(file_ref(m, idx, rel.to_owned(), abs))
        })
    }
}

/// Whether [`entry_name`] normalises names on this platform: on macOS.
pub const NFC_NAMES: bool = cfg!(target_os = "macos");

/// The name of a directory entry (or a path) as the build uses it (in [`FileRef::rel`], so in
/// paths, URLs and keys, and for the ignore rules and filters): NFC-normalised when `nfc`
/// ([`NFC_NAMES`]), as Go's `normalizeFilename` does on darwin, where HFS+ stores names
/// decomposed (NFD) and APFS keeps the form they were created in. [`FileRef::abs`] keeps the
/// name the OS returned, so the file is read, and matched with the watcher's events, by it;
/// what compares names read from the file system with `rel` (the static copy's
/// `cleanDestinationDir`) passes them through this first.
#[must_use]
pub fn entry_name(name: &str, nfc: bool) -> Cow<'_, str> {
    if nfc {
        ssg_base::text::nfc(name)
    } else {
        Cow::Borrowed(name)
    }
}

fn file_ref(m: &Mount, idx: u16, rel: String, abs: PathBuf) -> FileRef {
    FileRef {
        component: m.component,
        rel,
        abs,
        mount_lang: m.lang_idx(),
        mount_idx: idx,
    }
}

/// The path of a mounted single file inside its component: the target below the component, or
/// the file name when the target is the component itself.
fn file_mount_rel(m: &Mount) -> String {
    if m.sub.is_empty() {
        m.abs
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        m.sub.clone()
    }
}

fn join_rel(sub: &str, rest: &str) -> String {
    if sub.is_empty() {
        rest.to_owned()
    } else {
        format!("{sub}/{rest}")
    }
}

#[cfg(test)]
mod tests {
    use super::entry_name;

    #[test]
    fn entry_names_are_nfc_on_macos_only() {
        let nfd = "cafe\u{301}/Cafe\u{301}.md";
        assert_eq!(entry_name(nfd, true), "caf\u{e9}/Caf\u{e9}.md");
        assert_eq!(entry_name(nfd, false), nfd);
        assert_eq!(entry_name("caf\u{e9}.md", true), "caf\u{e9}.md");
        assert_eq!(entry_name("plain.md", true), "plain.md");
    }
}
