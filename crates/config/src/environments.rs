//! The settings of each environment: the `[environments.<name>]` tables of a configuration.
//!
//! The table of the build's environment (`--environment`) is merged over the configuration as a
//! later file would be, and `environments` is then removed, so the other environments' tables
//! are not settings. The folders of a configuration directory other than `_default`
//! (`config/production/`, the Go build's environments) are not read: one that holds a
//! configuration file is an error that says where its settings go.

use std::path::Path;

use ssg_base::{Map, Value};

use crate::source::{self, Format};
use crate::{ConfigError, tree};

/// The root key of the environments' tables.
const KEY: &str = "environments";

/// Merges `[environments.<environment>]` of `root` (keys normalised) over it and removes the
/// `environments` table.
pub(crate) fn apply(root: &mut Map, environment: &str) -> Result<(), ConfigError> {
    let Some(all) = root.remove(KEY) else {
        return Ok(());
    };
    let Value::Map(all) = all else {
        return Err(ConfigError::invalid(
            KEY,
            "is not a table of environments ([environments.production])",
        ));
    };
    match all.get(&ssg_base::text::to_lower(environment)) {
        None => Ok(()),
        Some(Value::Map(settings)) => {
            tree::merge_deep(root, &tree::normalize_keys(settings));
            Ok(())
        }
        Some(_) => Err(ConfigError::invalid(
            format!("{KEY}.{environment}"),
            "is not a table of settings",
        )),
    }
}

/// An error for the first folder of `config_dir` other than `_default` that holds a
/// configuration file.
pub(crate) fn refuse_folders(config_dir: &Path) -> Result<(), ConfigError> {
    if !config_dir.is_dir() {
        return Ok(());
    }
    let io = |source| ConfigError::Io {
        path: config_dir.to_owned(),
        source,
    };
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(config_dir).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.is_dir() && path.file_name().is_some_and(|n| n != "_default") {
            dirs.push(path);
        }
    }
    dirs.sort();
    for dir in dirs {
        let mut files = Vec::new();
        source::collect_files(&dir, &mut files)?;
        if files.iter().any(|f| Format::from_path(f).is_some()) {
            let environment = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Err(ConfigError::EnvironmentFolder { dir, environment });
        }
    }
    Ok(())
}
