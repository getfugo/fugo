//! The project's `.env` files: `NAME=value` lines (API keys and other secrets) that templates
//! read with `get_env`. `.env` is read first, then `.env.<environment>` (`.env.production`,
//! `.env.development`, …), whose lines win. They are not configuration: their values are not
//! merged into the configuration, `config` does not print them, and names that start with the
//! program's prefix ([`env::PREFIX`], `FUGO`) are ignored with a warning (the program's own
//! variables, such as `FUGO_TIMINGS`, are read from the process environment only).
//!
//! The format is the common one: one `NAME=value` per line, an optional `export ` in front;
//! blank lines and lines starting with `#` are skipped. An unquoted value is trimmed and ends
//! at a `#` that starts it or follows a space or tab (a comment); `'single'` quotes keep the
//! text as written; `"double"` quotes read `\n`, `\r`, `\t`, `\"` and `\\`. A value is one
//! line, and `${NAME}` is not expanded. A later line wins over an earlier one with the same
//! name.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ssg_base::diag::{Diagnostic, Position};

use crate::{ConfigError, env};

/// The base file's name, in the project directory.
pub const FILE_NAME: &str = ".env";

/// The files read for `environment`, in order (a later file's lines win): `.env` and
/// `.env.<environment>`.
#[must_use]
pub fn file_names(environment: &str) -> [String; 2] {
    [FILE_NAME.to_owned(), format!("{FILE_NAME}.{environment}")]
}

/// The variables of a project's `.env` files. `Debug` shows the names, never the values.
#[derive(Clone, Default)]
pub struct EnvFile {
    paths: Vec<PathBuf>,
    vars: Arc<BTreeMap<String, String>>,
}

impl fmt::Debug for EnvFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvFile")
            .field("paths", &self.paths)
            .field("names", &self.vars.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl EnvFile {
    /// Reads `<project>/.env`, then `<project>/.env.<environment>` over it ([`file_names`]);
    /// a missing file has no variables.
    ///
    /// # Errors
    /// A file cannot be read, or a line is not `NAME=value`.
    pub fn load(
        project: &Path,
        environment: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<Self, ConfigError> {
        let mut out = Self::default();
        for name in file_names(environment) {
            let path = project.join(name);
            match std::fs::read_to_string(&path) {
                Ok(text) => out.extend(Self::parse(&text, &path, diagnostics)?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => return Err(ConfigError::Io { path, source }),
            }
        }
        Ok(out)
    }

    /// Adds `later`'s files and variables; its values win.
    fn extend(&mut self, later: Self) {
        self.paths.extend(later.paths);
        let vars = Arc::make_mut(&mut self.vars);
        for (k, v) in later.vars.iter() {
            vars.insert(k.clone(), v.clone());
        }
    }

    /// Parses `text`, the content of the file at `path`.
    ///
    /// # Errors
    /// A line that is not `NAME=value`: no `=`, a name that is not a variable name, an
    /// unterminated quote or text after the closing quote.
    pub fn parse(
        text: &str,
        path: &Path,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<Self, ConfigError> {
        let file: Arc<Path> = Arc::from(path);
        let mut vars = BTreeMap::new();
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let position = Position {
                file: Arc::clone(&file),
                line: u32::try_from(i + 1).unwrap_or(u32::MAX),
                col: u32::try_from(line.len() - trimmed.len() + 1).unwrap_or(u32::MAX),
            };
            let syntax = |message: String| ConfigError::Syntax {
                position: position.clone(),
                message,
            };
            let rest = trimmed
                .strip_prefix("export")
                .filter(|r| r.starts_with([' ', '\t']))
                .map_or(trimmed, str::trim_start);
            let Some((name, value)) = rest.split_once('=') else {
                return Err(syntax("expected NAME=value".to_owned()));
            };
            let name = name.trim_end();
            if !is_name(name) {
                return Err(syntax(format!(
                    "{name:?} is not a variable name (letters, digits and `_`, not starting with \
                     a digit)"
                )));
            }
            let value = parse_value(value.trim_start()).map_err(syntax)?;
            if name.starts_with(env::PREFIX) {
                diagnostics.push(
                    Diagnostic::warning(format!(
                        "{name} is ignored: {FILE_NAME} holds variables for get_env; {}_* \
                         variables are read from the process environment only",
                        env::PREFIX
                    ))
                    .with_id("env-file-prefix")
                    .at(position.clone()),
                );
                continue;
            }
            vars.insert(name.to_owned(), value);
        }
        Ok(Self {
            paths: vec![path.to_path_buf()],
            vars: Arc::new(vars),
        })
    }

    /// The files the variables were read from, in order (empty: the project has none).
    #[must_use]
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// The value of `name`, when the file defines it.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(String::as_str)
    }

    /// The defined names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.vars.keys().map(String::as_str)
    }

    /// Every variable, shared (what `get_env` reads).
    #[must_use]
    pub fn vars(&self) -> Arc<BTreeMap<String, String>> {
        Arc::clone(&self.vars)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }
}

/// `[A-Za-z_][A-Za-z0-9_]*`.
fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The value after `=` (leading whitespace removed).
fn parse_value(raw: &str) -> Result<String, String> {
    let Some(quote @ ('"' | '\'')) = raw.chars().next() else {
        let end = raw
            .char_indices()
            .find(|&(i, c)| c == '#' && (i == 0 || raw[..i].ends_with([' ', '\t'])))
            .map_or(raw.len(), |(i, _)| i);
        return Ok(raw[..end].trim_end().to_owned());
    };
    let mut out = String::new();
    let mut chars = raw.char_indices().skip(1);
    let end = loop {
        let Some((i, c)) = chars.next() else {
            return Err(format!("unterminated {quote} quote (a value is one line)"));
        };
        if c == quote {
            break i + 1;
        }
        if quote == '"' && c == '\\' {
            match chars.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 'r')) => out.push('\r'),
                Some((_, 't')) => out.push('\t'),
                Some((_, e @ ('"' | '\\'))) => out.push(e),
                Some((_, other)) => {
                    out.push('\\');
                    out.push(other);
                }
                None => return Err(format!("unterminated {quote} quote (a value is one line)")),
            }
        } else {
            out.push(c);
        }
    };
    let tail = raw[end..].trim();
    if tail.is_empty() || tail.starts_with('#') {
        Ok(out)
    } else {
        Err(format!("unexpected text after the closing quote: {tail:?}"))
    }
}
