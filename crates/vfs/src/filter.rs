//! Which files a walk sees: a mount's `includeFiles`/`excludeFiles` globs and the per-component
//! ignore rules.

use std::path::Path;

use regex::Regex;
use ssg_base::glob::{self, Glob, GlobError, GlobOpts};
use ssg_base::paths;

use crate::Component;
use crate::vfs::{NFC_NAMES, entry_name};

/// A mount's `includeFiles` and `excludeFiles`, matched (case-insensitively) against the path
/// below the mount source with a leading slash (`/posts/a.md`).
///
/// A file is kept when it matches an inclusion, else dropped when it matches an exclusion,
/// else kept only if there are no inclusions. A directory is also kept when it is one of the
/// directories leading to an inclusion pattern (`/docs` for `docs/**.md`), so walks reach the
/// included files.
#[derive(Clone, Debug)]
pub(crate) struct FileFilter {
    include: Vec<Glob>,
    include_dirs: Vec<Glob>,
    exclude: Vec<Glob>,
}

impl FileFilter {
    pub(crate) fn new(include: &[String], exclude: &[String]) -> Result<Option<Self>, GlobError> {
        if include.is_empty() && exclude.is_empty() {
            return Ok(None);
        }
        let compile = |p: &str| glob::compile(&with_leading_slash(p), GlobOpts::default());
        let mut f = Self {
            include: Vec::new(),
            include_dirs: Vec::new(),
            exclude: Vec::new(),
        };
        for p in include {
            let p = with_leading_slash(p);
            f.include.push(compile(&p)?);
            let dir = paths::parent(&p);
            let parts: Vec<&str> = dir.split('/').collect();
            for i in 1..=parts.len() {
                f.include_dirs
                    .push(compile(&format!("/{}", paths::join(&parts[..i])))?);
            }
        }
        for p in exclude {
            f.exclude.push(compile(p)?);
        }
        Ok(Some(f))
    }

    /// Whether `path` (below the mount source, leading slash) passes.
    pub(crate) fn admits(&self, path: &str, is_dir: bool) -> bool {
        if self.include.iter().any(|g| g.is_match(path)) {
            return true;
        }
        if is_dir && !self.include.is_empty() && self.include_dirs.iter().any(|g| g.is_match(path))
        {
            return true;
        }
        if self.exclude.iter().any(|g| g.is_match(path)) {
            return false;
        }
        self.include.is_empty()
    }
}

fn with_leading_slash(p: &str) -> String {
    if p.starts_with('/') {
        p.to_owned()
    } else {
        format!("/{p}")
    }
}

/// The ignore rules of a walk.
#[derive(Clone, Debug)]
pub(crate) struct IgnoreRules {
    /// `ignoreFiles`: regular expressions matched against absolute file names.
    patterns: Vec<Regex>,
    /// Whether the regular expressions see the NFC form of the file name ([`NFC_NAMES`]), as
    /// Go matches them against `meta.Filename`, which it normalises on darwin.
    nfc: bool,
}

impl IgnoreRules {
    pub(crate) fn new(patterns: &[String]) -> Result<Self, (String, regex::Error)> {
        let patterns = patterns
            .iter()
            .map(|p| Regex::new(p).map_err(|e| (p.clone(), e)))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            patterns,
            nfc: NFC_NAMES,
        })
    }

    /// Whether the walk of component `c` skips the entry `name` at `abs`.
    ///
    /// - content, data, i18n: names starting with `.` or `#` or ending with `~` (files and
    ///   directories), and `ignoreFiles` matches;
    /// - layouts: files whose name starts with `.` or ends with `~`;
    /// - assets, static, archetypes: nothing (the static copy keeps dotfiles).
    pub(crate) fn skips(&self, c: Component, name: &str, abs: &Path, is_dir: bool) -> bool {
        match c {
            Component::Content | Component::Data | Component::I18n => {
                name.starts_with(['.', '#'])
                    || name.ends_with('~')
                    || abs.to_str().is_some_and(|a| {
                        let a = entry_name(a, self.nfc);
                        self.patterns.iter().any(|r| r.is_match(&a))
                    })
            }
            Component::Layouts => !is_dir && (name.starts_with('.') || name.ends_with('~')),
            Component::Assets | Component::Static | Component::Archetypes => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::IgnoreRules;
    use crate::Component;

    /// `ignoreFiles` written in NFC matches a file whose name the OS gives decomposed (HFS+) when
    /// names are normalised (macOS).
    #[test]
    fn ignore_files_see_the_nfc_name_on_macos() {
        let mut rules = IgnoreRules::new(&["caf\u{e9}".to_owned()]).expect("valid regexp");
        let nfd = Path::new("/site/content/cafe\u{301}.md");
        rules.nfc = false;
        assert!(!rules.skips(Component::Content, "cafe\u{301}.md", nfd, false));
        rules.nfc = true;
        assert!(rules.skips(Component::Content, "caf\u{e9}.md", nfd, false));
    }
}
