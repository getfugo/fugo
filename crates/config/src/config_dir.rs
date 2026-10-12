//! The Go build's configuration directory (`config/_default/`, `config/production/`), which is
//! not read: the configuration is one file. A folder of `config/` that holds configuration
//! files is an error that says where each file's settings go in that file.

use std::path::{Path, PathBuf};

use crate::ConfigError;
use crate::source::{CONFIG_BASE_NAMES, Format};

/// The configuration directory, in a project's (or a theme's) directory.
pub(crate) const NAME: &str = "config";

/// The folder of the configuration directory that holds the settings of every environment;
/// the others are named after one environment.
const DEFAULTS: &str = "_default";

/// An error naming the configuration files in the folders of `dir`'s configuration directory,
/// if it has any.
pub(crate) fn refuse(dir: &Path) -> Result<(), ConfigError> {
    let config_dir = dir.join(NAME);
    if !config_dir.is_dir() {
        return Ok(());
    }
    let mut files = Vec::new();
    for entry in read_dir(&config_dir)? {
        if entry.is_dir() {
            collect_files(&entry, &mut files)?;
        }
    }
    files.retain(|f| Format::from_path(f).is_some());
    if files.is_empty() {
        return Ok(());
    }
    files.sort();
    let files = files
        .into_iter()
        .map(|f| {
            let rel = f.strip_prefix(&config_dir).unwrap_or(&f).to_path_buf();
            let table = table(&rel);
            (rel, table)
        })
        .collect();
    Err(ConfigError::ConfigDirectory {
        dir: config_dir,
        files,
    })
}

/// The table of the configuration file that the settings of `rel` (a file below the
/// configuration directory) belong in, dotted, as the Go build placed them: its folder's
/// environment (`production/` under `environments.production`, nothing for `_default/`), then
/// its name (`params.toml` under `params`, `menus.en.toml` under `languages.en.menus`,
/// `config.toml` at the root). Empty: the root.
fn table(rel: &Path) -> String {
    let mut key: Vec<String> = Vec::new();
    if let Some(folder) = rel.iter().next().and_then(|f| f.to_str())
        && folder != DEFAULTS
    {
        key.extend(["environments".to_owned(), folder.to_owned()]);
    }
    let stem = rel
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (name, lang) = match stem.split_once('.') {
        Some((name, lang)) => (name, Some(lang)),
        None => (stem.as_str(), None),
    };
    let name = if name == "menu" { "menus" } else { name };
    match lang {
        _ if CONFIG_BASE_NAMES.contains(&name) => {}
        None => key.push(name.to_owned()),
        Some(lang) => key.extend(["languages".to_owned(), lang.to_owned(), name.to_owned()]),
    }
    key.join(".")
}

fn read_dir(dir: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    let io = |source| ConfigError::Io {
        path: dir.to_owned(),
        source,
    };
    std::fs::read_dir(dir)
        .map_err(io)?
        .map(|entry| entry.map(|e| e.path()).map_err(io))
        .collect()
}

/// The files below `dir`, at any depth, into `out`.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ConfigError> {
    for path in read_dir(dir)? {
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}
