//! The effective mounts of a project: `[[module.mounts]]`, the default mounts of unconfigured
//! components, the JS config files, and the themes' mounts.

use std::path::{Path, PathBuf};

use ssg_base::paths;
use ssg_base::{Idx, LangIdx};
use ssg_config::{Config, MountConfig, Theme, ThemeMounts};

use crate::filter::FileFilter;
use crate::{Component, VfsError};

/// The module a mount belongs to. The project's mounts take precedence over the themes', and
/// earlier themes over later ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Module {
    Project,
    /// The n-th theme of `Config::themes` (the `theme` list, `[[module.imports]]` and their
    /// themes, in precedence order).
    Theme(u16),
}

/// The language of a mount's files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MountLang {
    /// No `lang`: the files are in the default language (unless the file name says otherwise).
    Default,
    Lang(LangIdx),
    /// A disabled language: the mount contributes nothing.
    Disabled,
}

/// One mount: a directory (or a single file) of the project or a theme placed at a path of a
/// component.
#[derive(Clone, Debug)]
pub struct Mount {
    pub component: Component,
    /// The source as configured, cleaned (`content`, `node_modules/bootstrap/scss`, or an
    /// absolute path).
    pub source: String,
    /// The target, cleaned (`content`, `assets/vendor`).
    pub target: String,
    /// The absolute source path.
    pub abs: PathBuf,
    /// The language key (`lang`), when set.
    pub lang: Option<String>,
    pub include_files: Vec<String>,
    pub exclude_files: Vec<String>,
    pub disable_watch: bool,
    pub module: Module,
    /// The target below the component directory (`""`, `vendor`).
    pub(crate) sub: String,
    pub(crate) mount_lang: MountLang,
    pub(crate) filter: Option<FileFilter>,
}

impl Mount {
    /// The target below the component directory (`""` for `content`, `vendor` for
    /// `assets/vendor`).
    #[must_use]
    pub fn target_dir(&self) -> &str {
        &self.sub
    }

    /// The language of the mount's files; `None` means the default language.
    #[must_use]
    pub fn lang_idx(&self) -> Option<LangIdx> {
        match self.mount_lang {
            MountLang::Lang(l) => Some(l),
            MountLang::Default | MountLang::Disabled => None,
        }
    }

    /// Whether the mount's language is disabled (its files are not part of the build).
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.mount_lang == MountLang::Disabled
    }
}

/// A mount before its language, target and filters are resolved.
struct Draft {
    source: String,
    target: String,
    abs: PathBuf,
    lang: Option<String>,
    include_files: Vec<String>,
    exclude_files: Vec<String>,
    disable_watch: bool,
    module: Module,
}

impl Draft {
    fn new(project: &Path, source: &str, target: &str, lang: Option<String>) -> Self {
        let source = clean(source);
        Self {
            abs: absolute(project, &source),
            target: clean(target),
            source,
            lang,
            include_files: Vec::new(),
            exclude_files: Vec::new(),
            disable_watch: false,
            module: Module::Project,
        }
    }

    fn configured(project: &Path, m: &MountConfig) -> Self {
        let mut d = Self::new(project, &m.source, &m.target, m.lang.clone());
        d.include_files.clone_from(&m.include_files);
        d.exclude_files.clone_from(&m.exclude_files);
        d.disable_watch = m.disable_watch;
        d
    }

    fn component(&self) -> Option<Component> {
        Component::parse(self.target.split('/').next().unwrap_or_default())
    }

    fn is_js_config(&self) -> bool {
        self.target.starts_with("assets/_jsconfig")
    }
}

/// `path` cleaned lexically; the empty path is `.`.
fn clean(path: &str) -> String {
    paths::clean(&path.replace('\\', "/"))
}

fn absolute(project: &Path, source: &str) -> PathBuf {
    let p = Path::new(source);
    if p.is_absolute() {
        p.to_path_buf()
    } else if source == "." {
        project.to_path_buf()
    } else {
        project.join(p)
    }
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Root files that are mounted to `assets/_jsconfig/<name>` unless a mount targets that
/// directory: `package.json` and `package.config.json`.
fn is_js_config_file(name: &str) -> bool {
    name == "package.json" || name == "package.config.json"
}

/// The effective mounts of `cfg`, in precedence order.
pub(crate) fn mounts(cfg: &Config) -> Result<Vec<Mount>, VfsError> {
    let project = cfg.project_dir.as_path();
    let mut drafts: Vec<Draft> = Vec::new();

    // Configured mounts; a missing source is skipped.
    for (index, m) in cfg.mounts.iter().enumerate() {
        let d = Draft::configured(project, m);
        if d.component().is_none() {
            return Err(VfsError::InvalidTarget {
                index,
                target: m.target.clone(),
            });
        }
        if d.abs.exists() {
            drafts.push(d);
        }
    }

    if !drafts.iter().any(Draft::is_js_config) {
        for n in js_config_files(project) {
            drafts.push(Draft::new(
                project,
                &n,
                &format!("assets/_jsconfig/{n}"),
                None,
            ));
        }
    }

    // Default mounts for the components no mount targets (the JS config mounts do not count).
    let configured: Vec<Component> = drafts
        .iter()
        .filter(|d| !d.is_js_config())
        .filter_map(Draft::component)
        .collect();
    let dirs = &cfg.dirs;
    for c in Component::ALL {
        if configured.contains(&c) {
            continue;
        }
        match c {
            Component::Content => {
                for s in &cfg.sites {
                    let dir = s.content_dir.as_ref().unwrap_or(&dirs.content);
                    let lang = (dir != &dirs.content).then(|| s.language.key.clone());
                    drafts.push(Draft::new(project, &path_str(dir), c.as_str(), lang));
                }
            }
            Component::Static => {
                for s in &cfg.sites {
                    let lang = cfg.multihost.then(|| s.language.key.clone());
                    for dir in s.static_dirs.as_ref().unwrap_or(&dirs.static_dirs) {
                        drafts.push(Draft::new(
                            project,
                            &path_str(dir),
                            c.as_str(),
                            lang.clone(),
                        ));
                    }
                }
            }
            _ => {
                let dir = match c {
                    Component::Data => &dirs.data,
                    Component::Layouts => &dirs.layouts,
                    Component::I18n => &dirs.i18n,
                    Component::Archetypes => &dirs.archetypes,
                    _ => &dirs.assets,
                };
                drafts.push(Draft::new(project, &path_str(dir), c.as_str(), None));
            }
        }
    }

    dedupe(&mut drafts);

    for (i, theme) in cfg.themes.iter().enumerate() {
        let module = Module::Theme(u16::try_from(i).unwrap_or(u16::MAX));
        let mut theme_drafts = theme_mounts(theme)?;
        for d in &mut theme_drafts {
            d.module = module;
        }
        dedupe(&mut theme_drafts);
        drafts.extend(theme_drafts);
    }

    drafts.into_iter().map(|d| resolve(cfg, d)).collect()
}

/// Drops repeated mounts (same source, target and language), keeping the first.
fn dedupe(drafts: &mut Vec<Draft>) {
    let mut seen = std::collections::BTreeSet::new();
    drafts.retain(|d| seen.insert((d.source.clone(), d.target.clone(), d.lang.clone())));
}

/// The mounts of a theme (module still unset): its configured mounts whose source exists
/// (below the theme's directory), or each component directory it has, then its JS config
/// files unless a mount targets `assets/_jsconfig`; nothing with `noMounts`.
fn theme_mounts(theme: &Theme) -> Result<Vec<Draft>, VfsError> {
    let dir = &theme.dir;
    if !dir.is_dir() {
        return Err(VfsError::ThemeNotFound {
            name: theme.path.clone(),
            dir: dir.clone(),
        });
    }
    let mut drafts: Vec<Draft> = match &theme.mounts {
        ThemeMounts::None => return Ok(Vec::new()),
        ThemeMounts::Configured(mounts) => {
            let mut out = Vec::new();
            for (index, m) in mounts.iter().enumerate() {
                let mut m = m.clone();
                // An absolute source is relative to the theme's directory too (Go joins it).
                m.source = m.source.trim_start_matches(['/', '\\']).to_owned();
                let d = Draft::configured(dir, &m);
                if d.component().is_none() {
                    return Err(VfsError::InvalidTarget {
                        index,
                        target: m.target.clone(),
                    });
                }
                if d.abs.exists() {
                    out.push(d);
                }
            }
            out
        }
        ThemeMounts::Components => THEME_COMPONENTS
            .into_iter()
            .filter(|c| dir.join(c.as_str()).is_dir())
            .map(|c| Draft::new(dir, c.as_str(), c.as_str(), None))
            .collect(),
    };
    if !drafts.iter().any(Draft::is_js_config) {
        drafts.extend(
            js_config_files(dir)
                .into_iter()
                .map(|n| Draft::new(dir, &n, &format!("assets/_jsconfig/{n}"), None)),
        );
    }
    Ok(drafts)
}

/// The order of a theme's default mounts (Go's, by name).
const THEME_COMPONENTS: [Component; 7] = [
    Component::Archetypes,
    Component::Assets,
    Component::Content,
    Component::Data,
    Component::I18n,
    Component::Layouts,
    Component::Static,
];

/// The JS config files in `dir` (see [`is_js_config_file`]), sorted.
fn js_config_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| is_js_config_file(n))
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn resolve(cfg: &Config, d: Draft) -> Result<Mount, VfsError> {
    let (component, sub) = match d.target.split_once('/') {
        Some((c, rest)) => (c, rest.to_owned()),
        None => (d.target.as_str(), String::new()),
    };
    let component = Component::parse(component).ok_or_else(|| VfsError::InvalidTarget {
        index: 0,
        target: d.target.clone(),
    })?;
    let mount_lang = match &d.lang {
        None => MountLang::Default,
        Some(key) if key.is_empty() => MountLang::Default,
        Some(key) => match cfg.sites.iter().position(|s| &s.language.key == key) {
            Some(i) => MountLang::Lang(LangIdx::from_index(i)),
            None if cfg.disabled_languages.contains(key) => MountLang::Disabled,
            None => {
                return Err(VfsError::UnknownLanguage {
                    mount: d.source,
                    lang: key.clone(),
                });
            }
        },
    };
    let filter =
        FileFilter::new(&d.include_files, &d.exclude_files).map_err(|error| VfsError::Glob {
            mount: d.source.clone(),
            error,
        })?;
    Ok(Mount {
        component,
        source: d.source,
        target: d.target,
        abs: d.abs,
        lang: d.lang.filter(|l| !l.is_empty()),
        include_files: d.include_files,
        exclude_files: d.exclude_files,
        disable_watch: d.disable_watch,
        module: d.module,
        sub,
        mount_lang,
        filter,
    })
}
