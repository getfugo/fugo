//! The settings of each environment: the `[environments.<name>]` tables of a configuration.
//!
//! The table of the build's environment (`--environment`) is merged over the configuration as a
//! later file would be, and `environments` is then removed, so the other environments' tables
//! are not settings. The Go build's environment folders (`config/production/`) are not read
//! ([`crate::config_dir`]).

use ssg_base::{Map, Value};

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
