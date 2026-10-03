//! Content discovery (phase A3): the content component's files with their [`PathInfo`], leaf
//! bundles resolved, disabled languages dropped and duplicate keys settled.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use ssg_base::paths::ContentKey;
use ssg_base::{Idx, LangIdx};

use crate::mount::Module;
use crate::parser::{BundleKind, Parsed, PathInfo, PathParser};
use crate::{Component, FileRef, Vfs, VfsError};

/// A content file and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentFile {
    pub file: FileRef,
    pub info: PathInfo,
    /// The language named in the file name, else the mount's, else the default language.
    pub lang: LangIdx,
}

/// Two files with the same key and language; only the first is kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duplicate {
    pub key: ContentKey,
    pub lang: LangIdx,
    pub kept: PathBuf,
    pub dropped: PathBuf,
}

/// The result of [`Vfs::discover_content`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovery {
    /// Pages and resources, sorted by key, then language.
    pub files: Vec<ContentFile>,
    /// Content adapters (`_content.html`, `_content.gotmpl`), sorted by key (their directory),
    /// then language. They are no pages: an adapter shares its directory with the section's
    /// `_index.md` (Go keeps them in a tree of their own).
    pub adapters: Vec<ContentFile>,
    /// Files dropped because another file has the same key and language (for adapters:
    /// another adapter).
    pub duplicates: Vec<Duplicate>,
}

impl Vfs {
    /// Walks the content component and classifies every file:
    ///
    /// - files whose name carries a disabled language are dropped;
    /// - a directory whose first file is a leaf bundle index (`index.md`, `index.th.md`; a
    ///   branch index or a content file with a higher-ranked suffix comes first) is a leaf
    ///   bundle: every other file below it becomes a resource ([`PathInfo::into_bundled`]),
    ///   except the index files of other languages in the bundle directory itself;
    /// - of several pages (or several resources) with the same key and language the first is
    ///   kept: a bundle index before a single page (`foo/_index.md` before `foo.md`), then the
    ///   higher-ranked suffix (`md` before `html`), the earlier mount, a language named in the
    ///   file name before the mount's language, and the path. The others are reported as
    ///   [`Duplicate`]s;
    /// - content adapters go to [`Discovery::adapters`], one per directory and language (the
    ///   first by the same order).
    ///
    /// # Errors
    /// See [`Vfs::walk`].
    pub fn discover_content(&self, parser: &PathParser) -> Result<Discovery, VfsError> {
        let mut files: Vec<ContentFile> = self
            .walk(Component::Content)?
            .into_iter()
            .filter_map(|file| match parser.parse(Component::Content, &file.rel) {
                Parsed::DisabledLanguage => None,
                Parsed::File(info) => {
                    let lang = info
                        .lang
                        .or(file.mount_lang)
                        .unwrap_or_else(|| LangIdx::from_index(0));
                    Some(ContentFile {
                        file,
                        info: *info,
                        lang,
                    })
                }
            })
            .collect();

        let leaf_dirs = self.leaf_bundle_dirs(&files);
        if !leaf_dirs.is_empty() {
            files = files
                .into_iter()
                .map(|mut f| {
                    let dir = rel_dir(&f.file.rel);
                    let Some(root) = ancestors(dir).find(|d| leaf_dirs.contains(*d)) else {
                        return f;
                    };
                    let bundle_index = root == dir
                        && matches!(f.info.kind, BundleKind::Leaf | BundleKind::ContentAdapter);
                    if !bundle_index {
                        f.info = f.info.into_bundled();
                    }
                    f
                })
                .collect();
        }

        files.sort_by(|a, b| {
            (&a.info.key, !a.info.kind.is_page(), a.lang)
                .cmp(&(&b.info.key, !b.info.kind.is_page(), b.lang))
                .then_with(|| self.precedence(a).cmp(&self.precedence(b)))
        });
        let (adapters, files): (Vec<ContentFile>, Vec<ContentFile>) = files
            .into_iter()
            .partition(|f| f.info.kind == BundleKind::ContentAdapter);
        let mut out = Discovery::default();
        for (list, all) in [(&mut out.files, files), (&mut out.adapters, adapters)] {
            for f in all {
                match list.last() {
                    Some(k)
                        if k.info.key == f.info.key
                            && k.lang == f.lang
                            && k.info.kind.is_page() == f.info.kind.is_page() =>
                    {
                        out.duplicates.push(Duplicate {
                            key: f.info.key.clone(),
                            lang: f.lang,
                            kept: k.file.abs.clone(),
                            dropped: f.file.abs,
                        });
                    }
                    _ => list.push(f),
                }
            }
        }
        Ok(out)
    }

    fn module(&self, f: &ContentFile) -> Module {
        self.mounts()
            .get(usize::from(f.file.mount_idx))
            .map_or(Module::Project, |m| m.module)
    }

    /// The order of files with the same key: bundles first, then suffix (descending), mount,
    /// file-name language, path.
    fn precedence<'a>(&self, f: &'a ContentFile) -> Rank<'a> {
        (
            self.module(f),
            !f.info.kind.is_bundle(),
            Reverse(f.info.ext.as_str()),
            None,
            f.file.mount_idx,
            f.info.lang.is_none(),
            f.file.rel.as_str(),
        )
    }

    /// The order of a directory's files (the first decides whether it is a leaf bundle): like
    /// [`Self::precedence`], with the key after the suffix.
    fn dir_rank<'a>(&self, f: &'a ContentFile) -> Rank<'a> {
        let mut r = self.precedence(f);
        r.3 = Some(&f.info.key);
        r
    }

    /// The directories (component-relative, `""` for the root) whose first file (by
    /// [`Self::dir_rank`]) is a leaf bundle index; content adapters take no part.
    fn leaf_bundle_dirs(&self, files: &[ContentFile]) -> BTreeSet<String> {
        let mut firsts: BTreeMap<&str, &ContentFile> = BTreeMap::new();
        for f in files {
            if f.info.kind == BundleKind::ContentAdapter {
                continue;
            }
            let dir = rel_dir(&f.file.rel);
            match firsts.get(dir) {
                Some(first) if self.dir_rank(first) <= self.dir_rank(f) => {}
                _ => {
                    firsts.insert(dir, f);
                }
            }
        }
        firsts
            .into_iter()
            .filter(|(_, f)| f.info.kind == BundleKind::Leaf)
            .map(|(d, _)| d.to_owned())
            .collect()
    }
}

/// Module, not a bundle, suffix (descending), key, mount, no file-name language, path.
type Rank<'a> = (
    Module,
    bool,
    Reverse<&'a str>,
    Option<&'a ContentKey>,
    u16,
    bool,
    &'a str,
);

/// The directory of a component-relative path (`""` at the root).
fn rel_dir(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(d, _)| d)
}

/// `""`, then each ancestor of `dir`, then `dir` itself.
fn ancestors(dir: &str) -> impl Iterator<Item = &str> {
    std::iter::once("").chain(
        dir.match_indices('/')
            .map(move |(i, _)| &dir[..i])
            .chain((!dir.is_empty()).then_some(dir)),
    )
}
