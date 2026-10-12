//! The Go build's configuration directories (`config/_default/`, `config/production/`) in the
//! oracles' sites, folded into the one configuration file this port reads.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ssg_base::{Map, Params, Value};

/// The folder of a configuration directory that holds the settings of every environment.
const DEFAULTS: &str = "_default";

/// The configuration file names, in the Go build's lookup order.
const CONFIG_FILES: [&str; 4] = ["config.toml", "config.yaml", "config.yml", "config.json"];

/// Folds the configuration directory of `site` (`config_dir`, relative to `site`) into the
/// site's configuration file, and that of each theme (`themes/*/config`) into the theme's
/// ([`fold_dir`]).
///
/// # Errors
/// A configuration file that does not parse.
pub fn fold(site: &Path, config_dir: &str) -> Result<(), String> {
    fold_dir(site, config_dir)?;
    for theme in std::fs::read_dir(site.join("themes")).into_iter().flatten() {
        fold_dir(&theme.expect("theme").path(), "config")?;
    }
    Ok(())
}

/// Replaces `dir`'s configuration directory (`config_dir`, relative to `dir`) and its
/// configuration file with one file, `config.json`, that holds their settings as the Go build
/// merged them: the file's, then those of `_default/`, then each environment folder's under
/// `environments.<folder>` (the table the build's environment merges over the rest). Each
/// file of a folder is placed by its name (`params.toml` under `params`, `menus.en.toml` under
/// `languages.en.menus`, `config.*` at the root), in path order; keys are lower case, a later
/// file's tables merge into an earlier one's, and one with `_merge = "none"` replaces it. A
/// directory without configuration files is left as it is.
///
/// # Errors
/// A configuration file that does not parse.
pub fn fold_dir(dir: &Path, config_dir: &str) -> Result<(), String> {
    let config_dir = dir.join(config_dir);
    let Ok(entries) = std::fs::read_dir(&config_dir) else {
        return Ok(());
    };
    let mut folders: Vec<PathBuf> = entries
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_dir())
        .collect();
    // `_default` first, then the environments.
    folders.sort_by_key(|f| (!f.ends_with(DEFAULTS), f.clone()));
    let mut placed: Vec<(Vec<String>, PathBuf)> = Vec::new();
    for folder in &folders {
        let mut files = Vec::new();
        collect(folder, &mut files);
        files.sort();
        let environment = folder.file_name().expect("name").to_string_lossy();
        for file in files.into_iter().filter(|f| format(f).is_some()) {
            let mut prefix = Vec::new();
            if environment != DEFAULTS {
                prefix.extend(["environments".to_owned(), environment.to_lowercase()]);
            }
            let stem = file.file_stem().expect("stem").to_string_lossy();
            prefix.extend(file_prefix(&stem.to_lowercase()));
            placed.push((prefix, file));
        }
    }
    if placed.is_empty() {
        return Ok(());
    }
    let mut merged = Map::new();
    let root_files: Vec<PathBuf> = CONFIG_FILES
        .iter()
        .map(|name| dir.join(name))
        .filter(|p| p.is_file())
        .collect();
    if let Some(file) = root_files.first() {
        merge(&mut merged, &read(file)?);
    }
    for (prefix, file) in &placed {
        let settings = Value::map(read(file)?);
        let tree = prefix.iter().rev().fold(settings, |v, k| {
            Value::map(Map::from_iter([(k.as_str(), v)]))
        });
        merge(&mut merged, tree.as_map().expect("a table"));
    }
    for file in root_files {
        std::fs::remove_file(file).expect("remove the configuration file");
    }
    for folder in folders {
        std::fs::remove_dir_all(folder).expect("remove the folder");
    }
    let out = serde_json::to_string(&Value::map(merged)).expect("json");
    std::fs::write(dir.join("config.json"), out).expect("write");
    Ok(())
}

/// Merges `over` into `base` (keys lower case): tables merge key by key unless `over`'s has
/// `_merge = "none"`, anything else replaces.
fn merge(base: &mut Map, over: &Map) {
    for (k, v) in over.iter() {
        match (base.get_mut(k), v) {
            (Some(Value::Map(a)), Value::Map(b)) if !replaces(b) => merge(Arc::make_mut(a), b),
            _ => {
                base.insert(k, v.clone());
            }
        }
    }
}

fn replaces(m: &Map) -> bool {
    m.get("_merge")
        .and_then(Value::as_str)
        .is_some_and(|s| s.eq_ignore_ascii_case("none"))
}

/// The format of a configuration file, by its extension.
fn format(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_string_lossy().to_lowercase().as_str() {
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "json" => Some("json"),
        _ => None,
    }
}

/// The settings of a configuration file, keys lower case.
fn read(path: &Path) -> Result<Map, String> {
    let text = std::fs::read_to_string(path).expect("read");
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    let v = match format(path).expect("a configuration file") {
        "toml" => Value::from_toml_str(&text),
        "yaml" => Value::from_yaml_str(&text),
        _ => Value::from_json_str(&text),
    };
    match v.map_err(|e| format!("{}: {e}", path.display()))? {
        Value::Map(m) => Ok(Params::fold(&m).into_map()),
        Value::Null => Ok(Map::new()),
        _ => Err(format!("{}: not a table", path.display())),
    }
}

/// Where a configuration directory places a file named `stem`.
fn file_prefix(stem: &str) -> Vec<String> {
    let (name, lang) = match stem.split_once('.') {
        Some((name, lang)) => (name, Some(lang)),
        None => (stem, None),
    };
    let name = if name == "menu" { "menus" } else { name };
    match (name, lang) {
        ("config", _) => Vec::new(),
        (name, None) => vec![name.to_owned()],
        (name, Some(lang)) => vec!["languages".to_owned(), lang.to_owned(), name.to_owned()],
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}
