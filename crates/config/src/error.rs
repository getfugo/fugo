//! Configuration errors. Each one points at the file (and, where the format allows, the line
//! and column) the offending value came from.

use std::fmt;
use std::path::PathBuf;

use ssg_base::diag::Position;

/// Why the configuration could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A file or directory could not be read.
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Neither a configuration file nor a configuration directory was found.
    #[error("no configuration file (config.toml, config.yaml, config.yml or config.json) or config directory in {}", dir.display())]
    NotFound { dir: PathBuf },
    /// A theme (`theme`, `[[module.imports]]`) is not in the themes directory, in the project's
    /// `_vendor` directory or at the absolute path given.
    #[error("theme {name:?} not found: {} does not exist (themes are read from themesDir, _vendor or an absolute path; remote modules are not downloaded)", dir.display())]
    ThemeNotFound { name: String, dir: PathBuf },
    /// A theme of a theme is imported with an absolute path or a path outside the themes
    /// directory.
    #[error("theme {name:?} imported by {owner:?}: a theme's themes must be below themesDir ({})", themes_dir.display())]
    ThemeOutsideThemesDir {
        name: String,
        owner: String,
        themes_dir: PathBuf,
    },
    /// A file is not valid TOML, YAML or JSON.
    #[error("{position}: {message}")]
    Syntax { position: Position, message: String },
    /// A value does not fit its setting.
    #[error("{}", Located { key, position: position.as_ref(), message })]
    Invalid {
        /// The dotted key (`languages.en.weight`).
        key: String,
        /// Where the value was written, when it came from a file.
        position: Option<Position>,
        message: String,
    },
}

impl ConfigError {
    /// A value error without a known position (yet).
    pub(crate) fn invalid(key: impl Into<String>, message: impl fmt::Display) -> Self {
        Self::Invalid {
            key: key.into(),
            position: None,
            message: message.to_string(),
        }
    }

    /// The position the error points at, if known.
    #[must_use]
    pub fn position(&self) -> Option<&Position> {
        match self {
            Self::Syntax { position, .. } => Some(position),
            Self::Invalid { position, .. } => position.as_ref(),
            Self::Io { .. }
            | Self::NotFound { .. }
            | Self::ThemeNotFound { .. }
            | Self::ThemeOutsideThemesDir { .. } => None,
        }
    }
}

struct Located<'a> {
    key: &'a str,
    position: Option<&'a Position>,
    message: &'a str,
}

impl fmt::Display for Located<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(p) = self.position {
            write!(f, "{p}: ")?;
        }
        if self.key.is_empty() {
            f.write_str(self.message)
        } else {
            write!(f, "{}: {}", self.key, self.message)
        }
    }
}
