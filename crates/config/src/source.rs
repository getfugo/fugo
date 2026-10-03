//! Finding and decoding configuration files (step 2 of the A1 pipeline), and mapping a key
//! back to the line it was written on.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ssg_base::diag::{Diagnostic, Position};
use ssg_base::value::DecodeError;
use ssg_base::{Map, Value};

use crate::error::ConfigError;
use crate::tree;

/// The base names of a configuration file: `config`. In a configuration directory it places its
/// content at the root. The Go program's configuration file name is not read.
pub const CONFIG_BASE_NAMES: [&str; 1] = ["config"];

/// The configuration file extensions, in lookup order.
pub const CONFIG_EXTENSIONS: [&str; 4] = ["toml", "yaml", "yml", "json"];

/// The configuration file names searched in a project (or theme) directory, in lookup order;
/// the first that exists is read.
pub fn config_file_names() -> impl Iterator<Item = String> {
    CONFIG_BASE_NAMES
        .into_iter()
        .flat_map(|base| CONFIG_EXTENSIONS.map(|ext| format!("{base}.{ext}")))
}

/// The configuration file of `dir` (the first of [`config_file_names`] that exists) and, when
/// other names of that list exist as well, a warning naming the file read and the ones ignored.
pub fn find_config_file(dir: &Path) -> (Option<PathBuf>, Option<Diagnostic>) {
    let mut found = config_file_names().filter(|name| dir.join(name).is_file());
    let Some(used) = found.next() else {
        return (None, None);
    };
    let ignored: Vec<String> = found.collect();
    let path = dir.join(&used);
    let warning = (!ignored.is_empty()).then(|| {
        Diagnostic::warning(format!(
            "using {used}; ignoring {} (the first of config.toml, config.yaml, config.yml and \
             config.json is read)",
            ignored.join(", ")
        ))
        .with_id("config-file-ignored")
        .at(Position {
            file: Arc::from(path.as_path()),
            line: 0,
            col: 0,
        })
    });
    (Some(path), warning)
}

/// A configuration file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Toml,
    Yaml,
    Json,
}

impl Format {
    /// The format of a file with this extension.
    #[must_use]
    pub fn from_path(p: &Path) -> Option<Self> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "toml" => Some(Self::Toml),
            "yaml" | "yml" => Some(Self::Yaml),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

/// A decoded configuration file and where its keys land in the configuration tree.
#[derive(Debug)]
pub struct Source {
    pub path: Arc<Path>,
    format: Format,
    text: String,
    /// The key path the file's content is placed under (`["languages", "en", "menus"]` for
    /// `menus.en.toml`); empty for the root.
    pub prefix: Vec<String>,
    /// The file's content as written (keys not folded).
    pub tree: Map,
}

impl Source {
    /// Reads and decodes `path`.
    pub fn read(path: &Path, format: Format, prefix: Vec<String>) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_owned(),
            source,
        })?;
        Self::parse(path, format, text, prefix)
    }

    /// Decodes `text` as the content of `path`.
    pub fn parse(
        path: &Path,
        format: Format,
        text: String,
        prefix: Vec<String>,
    ) -> Result<Self, ConfigError> {
        let path: Arc<Path> = Arc::from(path);
        let decoded = if text.trim().is_empty() {
            Ok(Value::map(Map::new()))
        } else {
            match format {
                Format::Toml => Value::from_toml_str(&text),
                Format::Yaml => Value::from_yaml_str(&text),
                Format::Json => Value::from_json_str(&text),
            }
        };
        let tree = match decoded {
            Ok(Value::Map(m)) => Arc::unwrap_or_clone(m),
            Ok(Value::Null) => Map::new(),
            Ok(_) => {
                return Err(ConfigError::Syntax {
                    position: Position {
                        file: path,
                        line: 1,
                        col: 1,
                    },
                    message: "the document is not a table".to_owned(),
                });
            }
            Err(e) => return Err(syntax_error(&path, &text, &e)),
        };
        Ok(Self {
            path,
            format,
            text,
            prefix,
            tree,
        })
    }

    /// The content placed at its prefix.
    #[must_use]
    pub fn placed(&self) -> Map {
        let mut m = Map::new();
        if self.prefix.is_empty() {
            return self.tree.clone();
        }
        let segs: Vec<&str> = self.prefix.iter().map(String::as_str).collect();
        tree::set_segments(&mut m, &segs, Value::map(self.tree.clone()));
        m
    }

    /// Where the value at `key` (lower-case segments from the configuration root) is written
    /// in this file, if it is, with the key as spelled there.
    #[must_use]
    pub fn locate(&self, key: &[String]) -> Option<Located> {
        let rest = key.strip_prefix(self.prefix.as_slice())?;
        // The key must exist in this file.
        let mut written: Vec<String> = self.prefix.clone();
        let mut cur: Option<&Value> = None;
        for seg in rest {
            let (name, v) = match cur {
                None => lookup(&self.tree, seg)?,
                Some(Value::Map(m)) => lookup(m, seg)?,
                Some(Value::Array(a)) => (seg.as_str(), a.get(seg.parse::<usize>().ok()?)?),
                Some(_) => return None,
            };
            written.push(name.to_owned());
            cur = Some(v);
        }
        let offset = match self.format {
            Format::Toml => toml_offset(&self.text, rest),
            Format::Yaml | Format::Json => text_offset(&self.text, rest),
        };
        let (line, col) = offset.map_or((0, 0), |o| line_col(&self.text, o));
        Some(Located {
            position: Position {
                file: Arc::clone(&self.path),
                line,
                col,
            },
            key: written,
        })
    }
}

/// A key found in a source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    pub position: Position,
    /// The key path as written (indices as decimal strings).
    pub key: Vec<String>,
}

impl Located {
    /// The key as a dotted path (`languages.th.pagination.pagerSize`, `related.indices[1]`).
    #[must_use]
    pub fn dotted_key(&self) -> String {
        crate::de::DeError {
            path: self.key.clone(),
            message: String::new(),
        }
        .dotted_path()
    }
}

fn lookup<'a>(m: &'a Map, seg: &str) -> Option<(&'a str, &'a Value)> {
    m.iter().find(|(k, _)| k.eq_ignore_ascii_case(seg))
}

fn syntax_error(path: &Arc<Path>, text: &str, e: &DecodeError) -> ConfigError {
    let (line, col, message) = match e {
        DecodeError::Toml(t) => {
            let (l, c) = t.span().map_or((0, 0), |s| line_col(text, s.start));
            (l, c, t.message().to_owned())
        }
        DecodeError::Yaml(y) => {
            let (l, c) = y.location().map_or((0, 0), |loc| {
                (
                    u32::try_from(loc.line()).unwrap_or(0),
                    u32::try_from(loc.column()).unwrap_or(0),
                )
            });
            (l, c, first_line(&y.to_string()))
        }
        DecodeError::Json(j) => (
            u32::try_from(j.line()).unwrap_or(0),
            u32::try_from(j.column()).unwrap_or(0),
            first_line(&j.to_string()),
        ),
    };
    ConfigError::Syntax {
        position: Position {
            file: Arc::clone(path),
            line,
            col,
        },
        message,
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or_default().to_owned()
}

/// 1-based line and column (in characters) of byte `offset`.
fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(text.len());
    let before = &text[..text.floor_char_boundary(offset)];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(col).unwrap_or(u32::MAX),
    )
}

/// The byte offset of the key (or array element) at `path` in a TOML document.
fn toml_offset(text: &str, path: &[String]) -> Option<usize> {
    use toml::de::{DeTable, DeValue};
    let doc = DeTable::parse(text).ok()?;
    let mut table: &DeTable<'_> = doc.get_ref();
    let mut value: Option<&toml::Spanned<DeValue<'_>>> = None;
    let mut offset = None;
    for seg in path {
        if let Some(v) = value {
            match v.get_ref() {
                DeValue::Table(t) => table = t,
                DeValue::Array(a) => {
                    let i: usize = seg.parse().ok()?;
                    let item = a.get(i)?;
                    offset = Some(item.span().start);
                    value = Some(item);
                    continue;
                }
                _ => return offset,
            }
        }
        let (k, v) = table
            .iter()
            .find(|(k, _)| k.get_ref().eq_ignore_ascii_case(seg))?;
        offset = Some(k.span().start);
        value = Some(v);
    }
    offset
}

/// A best-effort byte offset of `path` in a YAML or JSON document: each key is searched after
/// the previous one.
fn text_offset(text: &str, path: &[String]) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    let mut from = 0;
    let mut found = None;
    for seg in path {
        if seg.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let seg = seg.to_ascii_lowercase();
        let at = lower[from..].find(&seg)? + from;
        found = Some(at);
        from = at + seg.len();
    }
    found
}

/// One configuration source in precedence order (a later source overrides an earlier one).
#[derive(Debug, Default)]
pub struct Sources {
    pub files: Vec<Source>,
}

impl Sources {
    /// The position of the value at `key` in the source with the highest precedence that
    /// defines it.
    #[must_use]
    pub fn locate(&self, key: &[String]) -> Option<Located> {
        self.files.iter().rev().find_map(|s| s.locate(key))
    }

    /// The merged tree of all files (not normalised).
    #[must_use]
    pub fn merged(&self) -> Map {
        let mut m = Map::new();
        for s in &self.files {
            tree::merge_deep(&mut m, &tree::normalize_keys(&s.placed()));
        }
        m
    }
}

/// The configuration file(s) of the project: the explicit list (the first file has the
/// highest precedence), or the first of [`config_file_names`] that exists (a warning goes to
/// `diagnostics` when there are several).
pub fn project_files(
    project: &Path,
    explicit: &[PathBuf],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<Source>, ConfigError> {
    if !explicit.is_empty() {
        let mut out = Vec::with_capacity(explicit.len());
        for f in explicit.iter().rev() {
            let mut path = project.join(f);
            if !path.is_file() && Format::from_path(&path).is_none() {
                // `--config custom` finds `custom.toml`, `custom.yaml`, …
                if let Some(p) = ["toml", "yaml", "yml", "json"]
                    .iter()
                    .map(|ext| path.with_extension(ext))
                    .find(|p| p.is_file())
                {
                    path = p;
                }
            }
            let format = Format::from_path(&path).unwrap_or(Format::Toml);
            if !path.is_file() {
                return Err(ConfigError::Io {
                    path,
                    source: std::io::Error::from(std::io::ErrorKind::NotFound),
                });
            }
            out.push(Source::read(&path, format, Vec::new())?);
        }
        return Ok(out);
    }
    let (found, warning) = find_config_file(project);
    diagnostics.extend(warning);
    match found {
        Some(path) => {
            let format = Format::from_path(&path).expect("known extension");
            Ok(vec![Source::read(&path, format, Vec::new())?])
        }
        None => Ok(Vec::new()),
    }
}

/// The files of one configuration directory (`config/_default`, `config/production`), in
/// path order, each placed by its file name: `config.*` at the root,
/// `params.en.*` under `languages.en.params`, `menus.en.*` under `languages.en.menus`, any
/// other `name.*` under `name`.
pub fn dir_files(dir: &Path) -> Result<Vec<Source>, ConfigError> {
    let mut paths = Vec::new();
    collect_files(dir, &mut paths)?;
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let Some(format) = Format::from_path(&path) else {
            continue;
        };
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        out.push(Source::read(&path, format, file_prefix(&stem))?);
    }
    Ok(out)
}

fn file_prefix(stem: &str) -> Vec<String> {
    let (name, lang) = match stem.split_once('.') {
        Some((name, lang)) => (name, Some(lang)),
        None => (stem, None),
    };
    let name = if name == "menu" { "menus" } else { name };
    match (name, lang) {
        (root, _) if CONFIG_BASE_NAMES.contains(&root) => Vec::new(),
        (name, None) => vec![name.to_owned()],
        (name, Some(lang)) => vec!["languages".to_owned(), lang.to_owned(), name.to_owned()],
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ConfigError> {
    let entries = std::fs::read_dir(dir).map_err(|source| ConfigError::Io {
        path: dir.to_owned(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ConfigError::Io {
            path: dir.to_owned(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}
