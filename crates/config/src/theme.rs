//! The project's themes (Go: modules imported with `theme` or `[[module.imports]]`), found
//! and read before their configuration is merged into the project's (the end of step 4 of the
//! A1 pipeline; the merge is [`crate::merge`]).
//!
//! - **Imports.** A configuration imports `[[module.imports]]` (`path`, `ignoreConfig`,
//!   `ignoreImports`, `noMounts`, `disable`, `mounts`), then the names of `theme`. The project's
//!   imports come first; each theme's own imports follow it (depth first), so `theme = ["a",
//!   "b"]` with `a` importing `c` gives `a`, `c`, `b`: the order of precedence. A theme imported
//!   twice (paths compared ignoring case and a `/vN` major version suffix) keeps its first
//!   place; `disable = true` skips an import.
//! - **Where.** `_vendor/<path>` when the importer's `_vendor/modules.txt` lists the path (the
//!   first listing wins, the closest with `module.vendorClosest`; `ignoreVendorPaths` is a glob
//!   of paths never looked up there), else `<themesDir>/<path>`, or the absolute path. The
//!   project's `module.replacements` (`"old -> new, …"`, a string or a list) renames
//!   `[[module.imports]]` paths. A theme of a theme must stay below `themesDir` unless its path
//!   was replaced. Remote modules are not downloaded: an import that is not found is an error.
//! - **Configuration.** The first `config.*` in the theme's directory, then its
//!   `config/_default/**` and `config/<environment>/**`, read like the project's; `theme.toml`
//!   (theme-site metadata) is not configuration.
//! - **Mounts.** The importer's `[[module.imports.mounts]]`, else the theme's own
//!   `[[module.mounts]]` (sources relative to the theme's directory), else each component
//!   directory the theme has ([`ThemeMounts`]); the file system layer mounts them after the
//!   project's.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ssg_base::diag::{Diagnostic, Position};
use ssg_base::glob::{self, Glob, GlobOpts};
use ssg_base::{Map, Value};

use crate::error::ConfigError;
use crate::global::{COMPONENTS, MountConfig};
use crate::source::{self, Format, Source, Sources};
use crate::{de, decode_error, tree};

/// A theme of the project, in precedence order (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Theme {
    /// The path as imported (`mytheme`, `github.com/me/theme`, an absolute path), after
    /// `module.replacements`.
    pub path: String,
    /// The theme's directory.
    pub dir: PathBuf,
    /// The theme that imported this one (`None`: the project).
    pub owner: Option<String>,
    /// The files its configuration was read from, lowest precedence first (none with
    /// `ignoreConfig`).
    pub config_files: Vec<PathBuf>,
    pub mounts: ThemeMounts,
    /// The version `_vendor/modules.txt` lists, when the theme was found in a `_vendor`
    /// directory.
    pub vendored: Option<String>,
}

/// What a theme contributes to the union file view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum ThemeMounts {
    /// Go's default: every component directory the theme has (`layouts`, `static`, …), and
    /// its JS config files.
    Components,
    /// The importer's `[[module.imports.mounts]]`, else the theme's own `[[module.mounts]]`;
    /// sources are relative to the theme's directory.
    Configured(Vec<MountConfig>),
    /// `noMounts = true` on the import: configuration only.
    None,
}

/// A `[[module.imports]]` entry as written.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors the configuration keys; converted to enums"
)]
struct RawImport {
    path: String,
    ignore_config: bool,
    ignore_imports: bool,
    no_mounts: bool,
    disable: bool,
    mounts: Vec<MountConfig>,
}

/// An import to follow: a `[[module.imports]]` entry that is not disabled, or a `theme` name.
#[derive(Clone, Debug)]
struct Import {
    path: String,
    /// The path was renamed by `module.replacements`: it may point outside `themesDir`.
    replaced: bool,
    reads: Reads,
    mounts: ImportMounts,
}

impl Import {
    fn theme(path: String) -> Self {
        Self {
            path,
            replaced: false,
            reads: Reads::All,
            mounts: ImportMounts::Own,
        }
    }
}

impl RawImport {
    /// The import, unless it is disabled.
    fn into_import(self) -> Option<Import> {
        let reads = if self.ignore_config {
            Reads::Nothing
        } else if self.ignore_imports {
            Reads::ConfigOnly
        } else {
            Reads::All
        };
        let mounts = if self.no_mounts {
            ImportMounts::None
        } else if self.mounts.is_empty() {
            ImportMounts::Own
        } else {
            ImportMounts::These(self.mounts)
        };
        (!self.disable).then_some(Import {
            path: self.path,
            replaced: false,
            reads,
            mounts,
        })
    }
}

/// What of an imported theme's configuration is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reads {
    /// Its configuration and its imports.
    All,
    /// Its configuration, not its imports (`ignoreImports`).
    ConfigOnly,
    /// Neither (`ignoreConfig`).
    Nothing,
}

/// The mounts an import asks for.
#[derive(Clone, Debug)]
enum ImportMounts {
    /// The theme's own: its `[[module.mounts]]`, else its component directories.
    Own,
    /// `[[module.imports.mounts]]`.
    These(Vec<MountConfig>),
    /// `noMounts`.
    None,
}

/// The themes of a project and their configuration.
#[derive(Debug, Default)]
pub(crate) struct Collected {
    /// In precedence order.
    pub themes: Vec<Theme>,
    /// Each theme's configuration files (in `themes` order).
    pub sources: Vec<Sources>,
    /// Each theme's configuration tree (its files merged, keys normalised).
    pub trees: Vec<Map>,
}

/// Finds the themes the project's configuration `root` (keys normalised) imports, and their
/// themes, and reads their configuration.
///
/// # Errors
/// A theme that is not found or is outside `themesDir`, an unreadable or invalid theme
/// configuration file or `_vendor/modules.txt`, invalid `module.replacements`,
/// `module.imports` or theme mounts.
pub(crate) fn collect(
    project: &Path,
    root: &Map,
    environment: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Collected, ConfigError> {
    let dirs = crate::Dirs::from_tree(root);
    let ignore_vendor = match root.get("ignorevendorpaths").and_then(de::weak_string) {
        Some(p) if !p.is_empty() => Some(
            glob::compile(p.trim_matches(['/', '.']), GlobOpts::default())
                .map_err(|e| ConfigError::invalid("ignoreVendorPaths", e))?,
        ),
        _ => None,
    };
    let replacements = replacements(root)?;
    let mut c = Collector {
        themes_dir: clean(&project.join(&dirs.themes)),
        environment,
        replacements,
        vendor: Vendor {
            closest: tree::get_path(root, "module.vendorclosest")
                .and_then(de::weak_bool)
                .unwrap_or(false),
            ignore: ignore_vendor,
            listed: BTreeMap::new(),
        },
        seen: BTreeSet::new(),
        out: Collected::default(),
        diagnostics,
    };
    let imports = c.imports(root, None)?;
    c.visit(None, project, imports)?;
    Ok(c.out)
}

struct Collector<'a> {
    themes_dir: PathBuf,
    environment: &'a str,
    /// `module.replacements`: old path → new path.
    replacements: BTreeMap<String, String>,
    vendor: Vendor,
    /// [`path_key`]s of the imports taken.
    seen: BTreeSet<String>,
    out: Collected,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Collector<'_> {
    /// The imports of a configuration tree: `[[module.imports]]` (renamed by
    /// `module.replacements`, disabled ones left out), then `theme`. `owner` is the theme the
    /// tree belongs to.
    fn imports(&self, tree: &Map, owner: Option<usize>) -> Result<Vec<Import>, ConfigError> {
        let raw: Vec<RawImport> = match tree::get_path(tree, "module.imports") {
            None | Some(Value::Null) => Vec::new(),
            Some(v) => de::from_value(v)
                .map_err(|e| self.locate(decode_error("module.imports", &e), owner))?,
        };
        let mut out = Vec::with_capacity(raw.len());
        for (i, r) in raw.into_iter().enumerate() {
            let Some(mut imp) = r.into_import() else {
                continue;
            };
            if let ImportMounts::These(m) = &imp.mounts {
                check_mounts(m, &format!("module.imports[{i}].mounts"))
                    .map_err(|e| self.locate(e, owner))?;
            }
            if let Some(new) = self.replacements.get(&imp.path) {
                imp.path.clone_from(new);
                imp.replaced = true;
            }
            out.push(imp);
        }
        let names: Vec<String> = match tree.get("theme") {
            Some(Value::Array(a)) => a.iter().filter_map(de::weak_string).collect(),
            Some(v) => de::weak_string(v).into_iter().collect(),
            None => Vec::new(),
        };
        out.extend(names.into_iter().map(Import::theme));
        Ok(out)
    }

    /// Adds `imports` of `owner` (whose directory is `owner_dir`) and, depth first, their
    /// imports.
    fn visit(
        &mut self,
        owner: Option<usize>,
        owner_dir: &Path,
        imports: Vec<Import>,
    ) -> Result<(), ConfigError> {
        for imp in imports {
            if imp.path.is_empty() || !self.seen.insert(path_key(&imp.path)) {
                continue;
            }
            let idx = self.add(owner, owner_dir, &imp)?;
            if imp.reads != Reads::All {
                continue;
            }
            let nested = self.imports(&self.out.trees[idx], Some(idx))?;
            let dir = self.out.themes[idx].dir.clone();
            self.visit(Some(idx), &dir, nested)?;
        }
        Ok(())
    }

    /// Finds the theme `imp` of `owner`, reads its configuration and decides its mounts.
    fn add(
        &mut self,
        owner: Option<usize>,
        owner_dir: &Path,
        imp: &Import,
    ) -> Result<usize, ConfigError> {
        let owner_path = owner.map(|i| self.out.themes[i].path.clone());
        let (dir, vendored) = self.find(owner_path.as_deref(), owner_dir, imp)?;
        let sources = match imp.reads {
            Reads::Nothing => Sources::default(),
            Reads::All | Reads::ConfigOnly => {
                read_config(&dir, self.environment, self.diagnostics)?
            }
        };
        let tree = sources.merged();
        let own_mounts: Vec<MountConfig> = match tree::get_path(&tree, "module.mounts") {
            None | Some(Value::Null) => Vec::new(),
            Some(v) => de::from_value(v)
                .map_err(|e| locate_in(&sources, decode_error("module.mounts", &e)))?,
        };
        let mounts = match &imp.mounts {
            ImportMounts::None => ThemeMounts::None,
            ImportMounts::These(m) => ThemeMounts::Configured(m.clone()),
            ImportMounts::Own if own_mounts.is_empty() => ThemeMounts::Components,
            ImportMounts::Own => {
                check_mounts(&own_mounts, "module.mounts").map_err(|e| locate_in(&sources, e))?;
                ThemeMounts::Configured(own_mounts)
            }
        };
        self.out.themes.push(Theme {
            path: imp.path.clone(),
            dir,
            owner: owner_path,
            config_files: sources.files.iter().map(|s| s.path.to_path_buf()).collect(),
            mounts,
            vendored,
        });
        self.out.sources.push(sources);
        self.out.trees.push(tree);
        Ok(self.out.themes.len() - 1)
    }

    /// The directory of the theme `imp` imported by `owner` (`None`: the project), and its
    /// `_vendor` version.
    fn find(
        &mut self,
        owner: Option<&str>,
        owner_dir: &Path,
        imp: &Import,
    ) -> Result<(PathBuf, Option<String>), ConfigError> {
        if !self.vendor.ignores(&imp.path) {
            self.vendor.read(owner_dir)?;
            if let Some((dir, version)) = self.vendor.listed.get(&imp.path) {
                if !dir.is_dir() {
                    return Err(ConfigError::ThemeNotFound {
                        name: imp.path.clone(),
                        dir: dir.clone(),
                    });
                }
                return Ok((dir.clone(), Some(version.clone())));
            }
        }
        let path = Path::new(&imp.path);
        let anywhere = owner.is_none() || imp.replaced;
        let dir = if path.is_absolute() {
            clean(path)
        } else {
            clean(&self.themes_dir.join(path))
        };
        if !anywhere && !dir.starts_with(&self.themes_dir) {
            return Err(ConfigError::ThemeOutsideThemesDir {
                name: imp.path.clone(),
                owner: owner.unwrap_or_default().to_owned(),
                themes_dir: self.themes_dir.clone(),
            });
        }
        if !dir.is_dir() {
            return Err(ConfigError::ThemeNotFound {
                name: imp.path.clone(),
                dir,
            });
        }
        Ok((dir, None))
    }

    /// Adds the position of the key of a value error in `owner`'s configuration (`None`: the
    /// project's, which the caller locates).
    fn locate(&self, e: ConfigError, owner: Option<usize>) -> ConfigError {
        match owner {
            Some(i) => locate_in(&self.out.sources[i], e),
            None => e,
        }
    }
}

/// `_vendor/modules.txt` listings: module path → (directory, version).
struct Vendor {
    /// `module.vendorClosest`: a listing closer to the importer wins over an earlier one.
    closest: bool,
    /// `ignoreVendorPaths`.
    ignore: Option<Glob>,
    listed: BTreeMap<String, (PathBuf, String)>,
}

impl Vendor {
    fn ignores(&self, path: &str) -> bool {
        self.ignore.as_ref().is_some_and(|g| g.is_match(path))
    }

    /// Reads `<dir>/_vendor/modules.txt` when it exists: lines `# <module path> <version>`.
    fn read(&mut self, dir: &Path) -> Result<(), ConfigError> {
        let vendor = dir.join("_vendor");
        let file = vendor.join("modules.txt");
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(source) => return Err(ConfigError::Io { path: file, source }),
        };
        for (i, line) in text.lines().enumerate() {
            let line = line.trim_matches(['#', ' ']).trim();
            if line.is_empty() {
                continue;
            }
            let [path, version] = line.split_whitespace().collect::<Vec<_>>()[..] else {
                return Err(ConfigError::Syntax {
                    position: Position {
                        file: Arc::from(file.as_path()),
                        line: u32::try_from(i + 1).unwrap_or(u32::MAX),
                        col: 1,
                    },
                    message: "expected \"# <module path> <version>\"".to_owned(),
                });
            };
            if self.closest || !self.listed.contains_key(path) {
                self.listed
                    .insert(path.to_owned(), (vendor.join(path), version.to_owned()));
            }
        }
        Ok(())
    }
}

/// A theme's configuration files: the first of `config.*`, `config.*` in `dir`,
/// then `config/_default/**` and `config/<environment>/**`.
fn read_config(
    dir: &Path,
    environment: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Sources, ConfigError> {
    let mut files = Vec::new();
    let (found, warning) = source::find_config_file(dir);
    diagnostics.extend(warning);
    if let Some(path) = found {
        let format = Format::from_path(&path).expect("a configuration file name");
        files.push(Source::read(&path, format, Vec::new())?);
    }
    for sub in ["_default", environment] {
        let d = dir.join("config").join(sub);
        if d.is_dir() {
            files.extend(source::dir_files(&d)?);
        }
    }
    Ok(Sources { files })
}

/// The project's `module.replacements`: `"old -> new"` pairs, comma-separated in a string (or
/// in a list's only element), or one per list element.
fn replacements(root: &Map) -> Result<BTreeMap<String, String>, ConfigError> {
    let items: Vec<String> = match tree::get_path(root, "module.replacements") {
        None | Some(Value::Null) => return Ok(BTreeMap::new()),
        Some(Value::Array(a)) => a.iter().filter_map(de::weak_string).collect(),
        Some(v) => de::weak_string(v).into_iter().collect(),
    };
    let items: Vec<String> = match items.as_slice() {
        [one] => one.split(',').map(str::to_owned).collect(),
        _ => items,
    };
    let mut out = BTreeMap::new();
    for item in items.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let Some((old, new)) = item.split_once("->").filter(|(_, new)| !new.contains("->")) else {
            return Err(ConfigError::invalid(
                "module.replacements",
                format_args!("{item:?} is not a replacement (\"old/path -> new/path\")"),
            ));
        };
        out.insert(old.trim().to_owned(), new.trim().to_owned());
    }
    Ok(out)
}

/// Every mount needs a source and a target under a component directory.
fn check_mounts(mounts: &[MountConfig], key: &str) -> Result<(), ConfigError> {
    for (i, m) in mounts.iter().enumerate() {
        if m.source.is_empty() {
            return Err(ConfigError::invalid(
                format!("{key}[{i}].source"),
                "a mount needs a source",
            ));
        }
        let component = m
            .target
            .trim_start_matches(['/', '\\'])
            .split(['/', '\\'])
            .next();
        if !component.is_some_and(|c| COMPONENTS.contains(&c)) {
            return Err(ConfigError::invalid(
                format!("{key}[{i}].target"),
                format_args!(
                    "{:?} is not under a component directory ({})",
                    m.target,
                    COMPONENTS.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

/// Adds the position of a value error's key in `sources` (the theme's configuration).
fn locate_in(sources: &Sources, e: ConfigError) -> ConfigError {
    match e {
        ConfigError::Invalid {
            key,
            position: None,
            message,
        } => match sources.locate(&crate::key_segments(&key)) {
            Some(found) => ConfigError::Invalid {
                key: found.dotted_key(),
                position: Some(found.position),
                message,
            },
            None => ConfigError::Invalid {
                key,
                position: None,
                message,
            },
        },
        other => other,
    }
}

/// The identity of an import path: lower case, without a `/vN` major version suffix (`N` ≥ 2,
/// as in Go module paths).
#[must_use]
pub fn path_key(path: &str) -> String {
    let major = |s: &str| {
        s.strip_prefix('v').is_some_and(|n| {
            !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && !n.starts_with('0')
                && n != "1"
        })
    };
    let prefix = match path.rsplit_once('/') {
        Some((prefix, last)) if !prefix.is_empty() && major(last) => prefix,
        _ => path,
    };
    prefix.to_lowercase()
}

/// `p` with `.` and `..` resolved lexically.
fn clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}
