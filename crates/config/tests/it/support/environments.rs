//! The Go build's environment folders (`config/production/`), recreated as this port reads the
//! same settings: `[environments.<name>]` tables of the configuration.

use std::path::{Path, PathBuf};

use ssg_base::{Map, Value};

/// Replaces each folder of `config_dir` other than `_default` with JSON files in
/// `config_dir/_default` (`config.environment-<n>.json`, root files) that hold its files'
/// settings under `environments.<folder>`, each placed as its file name placed it
/// (`params.toml` under `params`, `menus.en.toml` under `languages.en.menus`). The files keep
/// their path order.
pub fn move_folders(config_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(config_dir) else {
        return;
    };
    let mut folders: Vec<PathBuf> = entries
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_dir() && !p.ends_with("_default"))
        .collect();
    folders.sort();
    let mut n = 0;
    for folder in folders {
        let environment = folder
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let mut files = Vec::new();
        collect(&folder, &mut files);
        files.sort();
        for path in files {
            let Some(settings) = read(&path) else {
                continue;
            };
            let stem = path
                .file_stem()
                .expect("stem")
                .to_string_lossy()
                .to_lowercase();
            let mut prefix = vec!["environments".to_owned(), environment.clone()];
            prefix.extend(file_prefix(&stem));
            let placed = prefix.iter().rev().fold(settings, |v, k| {
                Value::map(Map::from_iter([(k.as_str(), v)]))
            });
            let out = config_dir.join(format!("_default/config.environment-{n:03}.json"));
            std::fs::create_dir_all(out.parent().expect("parent")).expect("dir");
            std::fs::write(&out, serde_json::to_string(&placed).expect("json")).expect("write");
            n += 1;
        }
        std::fs::remove_dir_all(&folder).expect("remove the folder");
    }
}

/// The settings of a configuration file; `None` for a file of another kind.
fn read(path: &Path) -> Option<Value> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    let text = std::fs::read_to_string(path).expect("read");
    if text.trim().is_empty() {
        return Some(Value::map(Map::new()));
    }
    let v = match ext.as_str() {
        "toml" => Value::from_toml_str(&text),
        "yaml" | "yml" => Value::from_yaml_str(&text),
        "json" => Value::from_json_str(&text),
        _ => return None,
    };
    Some(v.expect("a configuration file"))
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
