//! What the CMS may write (the areas every role's `edit` globs are cut down to), and where the
//! project is in its git repository.
//!
//! Paths are project-relative with `/` separators. The areas are the project's own content,
//! data and i18n directories, the `params` and `menus` files of `config/_default/`, and the
//! media directory, each with the file types it may hold. Nothing else is writable whatever a
//! role says: not the configuration files (mounts could publish `.env`), layouts, assets,
//! content adapters (`_content.*` are templates; the Worker matches [`DENY`] against the path as
//! the build keys it, lower case), workflows or dotfiles. HTML content files (raw pages on the
//! editor's own origin) only with `cms.html`.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use ssg_config::Config;

use crate::CmsError;
use crate::config::CmsConfig;

/// Files an upload may be (images, documents, audio, video). No SVG, HTML, CSS or scripts:
/// they would run on the site's origin, where the editor's API is.
pub const MEDIA_EXT: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "avif", "bmp", "tif", "tiff", "ico", "pdf", "mp4", "webm",
    "mov", "mp3", "m4a", "ogg", "wav",
];
/// Data and translation files.
pub const DATA_EXT: &[&str] = &["toml", "yaml", "yml", "json", "csv", "xml"];
/// The `params` and `menus` configuration files.
pub const CONFIG_EXT: &[&str] = &["toml", "yaml", "yml", "json"];
/// Paths no area covers, whatever the globs say: content adapters are templates. Lower case:
/// they are matched against the lower-cased path.
pub const DENY: &[&str] = &["**/_content.*"];
/// Content suffixes that are raw HTML pages, writable only with `cms.html`.
const HTML_EXT: &[&str] = &["html", "htm"];

/// A part of the project the CMS may write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Area {
    /// `content`, `data`, `i18n`, `config` or `media`.
    pub kind: &'static str,
    /// The glob of the area's files (`content/**`).
    pub glob: String,
    /// The file extensions allowed there (lower case, without the dot).
    pub ext: Vec<String>,
}

/// The areas of a project (see the module docs).
///
/// # Errors
/// A media directory outside the static, assets and content directories.
pub fn areas(cfg: &Config, cms: &CmsConfig) -> Result<Vec<Area>, CmsError> {
    let mut content_ext: Vec<String> = cfg
        .content_types
        .0
        .iter()
        .flat_map(|&id| cfg.media_types.get(id).suffixes.iter().cloned())
        .filter(|e| cms.html || !HTML_EXT.contains(&e.as_str()))
        .collect();
    content_ext.extend(MEDIA_EXT.iter().map(|&e| e.to_owned()));
    content_ext.sort();
    content_ext.dedup();
    let list = |exts: &[&str]| exts.iter().map(|&e| e.to_owned()).collect::<Vec<_>>();

    let mut out = Vec::new();
    for dir in content_dirs(cfg) {
        out.push(Area {
            kind: "content",
            glob: format!("{}/**", escape_glob(&dir)),
            ext: content_ext.clone(),
        });
    }
    if let Some(dir) = project_rel(cfg, &cfg.dirs.data) {
        out.push(Area {
            kind: "data",
            glob: format!("{}/**", escape_glob(&dir)),
            ext: list(DATA_EXT),
        });
    }
    if let Some(dir) = project_rel(cfg, &cfg.dirs.i18n) {
        out.push(Area {
            kind: "i18n",
            glob: format!("{}/**", escape_glob(&dir)),
            ext: list(DATA_EXT),
        });
    }
    out.push(Area {
        kind: "config",
        glob: format!(
            "{}/_default/{{params,menus,menu}}.*",
            escape_glob(&config_dir(cfg))
        ),
        ext: list(CONFIG_EXT),
    });
    if let Some(media) = &cms.media {
        let mut roots: Vec<String> = cfg
            .dirs
            .static_dirs
            .iter()
            .chain(std::iter::once(&cfg.dirs.assets))
            .filter_map(|d| project_rel(cfg, d))
            .collect();
        roots.extend(content_dirs(cfg));
        if !roots
            .iter()
            .any(|r| media.starts_with(&format!("{r}/")) || media == r)
        {
            return Err(CmsError::config(
                "cms.media",
                format!(
                    "{media:?} is not inside the static, assets or content directories ({})",
                    roots.join(", ")
                ),
            ));
        }
        out.push(Area {
            kind: "media",
            glob: format!("{}/**", escape_glob(media)),
            ext: list(MEDIA_EXT),
        });
    }
    Ok(out)
}

/// `s` with the glob syntax characters escaped, so a directory name matches only itself.
fn escape_glob(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '{' | '}' | ',' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The project's content directories: `contentDir`, then the languages' own, without
/// duplicates (project-relative).
pub fn content_dirs(cfg: &Config) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let dirs = std::iter::once(&cfg.dirs.content)
        .chain(cfg.sites.iter().filter_map(|s| s.content_dir.as_ref()));
    for d in dirs {
        if let Some(rel) = project_rel(cfg, d)
            && !out.contains(&rel)
        {
            out.push(rel);
        }
    }
    out
}

/// The configuration directory: the parent of the `_default/` directory the configuration was
/// read from, else `config`.
fn config_dir(cfg: &Config) -> String {
    cfg.config_files
        .iter()
        .filter_map(|f| f.parent())
        .filter(|d| d.file_name().is_some_and(|n| n == "_default"))
        .find_map(|d| d.parent().and_then(|p| project_rel(cfg, p)))
        .unwrap_or_else(|| "config".to_owned())
}

/// `path` relative to the project directory with `/` separators, if it is inside it (and not
/// the project directory itself).
pub fn project_rel(cfg: &Config, path: &Path) -> Option<String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cfg.project_dir.join(path)
    };
    let rel = normalize(&abs)?
        .strip_prefix(normalize(&cfg.project_dir)?)
        .ok()?
        .to_path_buf();
    let parts: Vec<String> = rel
        .components()
        .map(|c| match c {
            Component::Normal(s) => s.to_str().map(str::to_owned),
            _ => None,
        })
        .collect::<Option<_>>()?;
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `p` without `.` and `..` components (lexically; symbolic links are not followed).
fn normalize(p: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            other => out.push(other),
        }
    }
    Some(out)
}

/// Whether a role's glob can match a path of some area: their literal prefixes (up to the
/// last `/` before the first wildcard) must not lead to different directories.
pub fn reaches_an_area(glob: &str, areas: &[Area]) -> bool {
    let a = literal_dir(glob);
    areas.iter().any(|area| {
        let b = literal_dir(&area.glob);
        a.starts_with(b) || b.starts_with(a)
    })
}

fn literal_dir(glob: &str) -> &str {
    let end = glob.find(['*', '?', '[', '{', '\\']).unwrap_or(glob.len());
    let lit = &glob[..end];
    lit.rfind('/').map_or("", |i| &lit[..=i])
}

/// The project's directory in its git repository (`""` at the root, else `docs/`), and
/// whether a repository was found: `cms.git.dir` when set, else the path from the nearest
/// directory above the project that has a `.git`.
pub fn repo_dir(cfg: &Config, cms: &CmsConfig) -> (String, bool) {
    if let Some(dir) = &cms.git.dir {
        return (dir.clone(), true);
    }
    let project =
        std::fs::canonicalize(&cfg.project_dir).unwrap_or_else(|_| cfg.project_dir.clone());
    let mut dir = project.as_path();
    loop {
        if dir.join(".git").exists() {
            let rel = project.strip_prefix(dir).unwrap_or(Path::new(""));
            let parts: Vec<_> = rel
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => s.to_str(),
                    _ => None,
                })
                .collect();
            let prefix = if parts.is_empty() {
                String::new()
            } else {
                format!("{}/", parts.join("/"))
            };
            return (prefix, true);
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => return (String::new(), false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(glob: &str) -> Area {
        Area {
            kind: "content",
            glob: glob.to_owned(),
            ext: Vec::new(),
        }
    }

    #[test]
    fn directory_names_are_escaped_in_globs() {
        assert_eq!(escape_glob("content"), "content");
        assert_eq!(escape_glob("a*b/[x]{y,z}"), r"a\*b/\[x\]\{y\,z\}");
    }

    #[test]
    fn globs_reach_areas_by_their_literal_directories() {
        let areas = [
            area("content/**"),
            area("config/_default/{params,menus,menu}.*"),
        ];
        for ok in [
            "**",
            "*/x",
            "content/**",
            "content/blog/*.md",
            "config/_default/params.toml",
            "{content,data}/**",
        ] {
            assert!(reaches_an_area(ok, &areas), "{ok}");
        }
        for bad in [
            "layouts/**",
            "assets/js/*.js",
            "config/production/params.toml",
            ".github/**",
        ] {
            assert!(!reaches_an_area(bad, &areas), "{bad}");
        }
    }
}
