//! What the index reads from the content tree: the files and their front matter, the kinds of
//! values, and the sections.

use super::*;

/// The prefix front matter names a media file with: the URL path for a static directory
/// (`/images/uploads/`), the path below the directory for assets and content
/// (`images/uploads/`).
pub(super) fn media_ref(cfg: &Config, media: &str) -> Option<String> {
    let base = cfg.default_site().base_url.base_path();
    let under = |dir: &str| {
        media
            .strip_prefix(dir)
            .and_then(|r| r.strip_prefix('/'))
            .map(|r| format!("{r}/"))
    };
    for d in &cfg.dirs.static_dirs {
        if let Some(rest) = paths::project_rel(cfg, d).and_then(|d| under(&d)) {
            return Some(format!("{base}{rest}"));
        }
    }
    std::iter::once(&cfg.dirs.assets)
        .chain(std::iter::once(&cfg.dirs.content))
        .find_map(|d| paths::project_rel(cfg, d).and_then(|d| under(&d)))
}

/// The files under `dir` (relative, `/`-separated, sorted), without hidden files and
/// directories.
pub(super) fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    let mut items: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.collect::<Result<_, _>>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    items.sort_by_key(std::fs::DirEntry::file_name);
    for item in items {
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let ty = item.file_type()?;
        if ty.is_dir() {
            walk(&item.path(), &rel, out)?;
        } else if ty.is_file() || ty.is_symlink() {
            out.push(rel);
        }
    }
    Ok(())
}

/// The front matter format and keys of a content file (empty when it has none or it does not
/// decode: the editor shows such files as text).
pub(super) fn front_matter(src: &str) -> (&'static str, Map) {
    let Ok(split) = split_front_matter(src) else {
        return ("none", Map::new());
    };
    let Some((format, text)) = split.front_matter else {
        return ("none", Map::new());
    };
    let name = match format {
        FrontMatterFormat::Yaml => "yaml",
        FrontMatterFormat::Toml => "toml",
        FrontMatterFormat::Json => "json",
        FrontMatterFormat::Org => "org",
    };
    (
        name,
        decode_front_matter_map(format, text).unwrap_or_default(),
    )
}

/// The value of `key` in a case-preserving map, matched ignoring ASCII case.
pub(super) fn get_ci<'a>(m: &'a Map, key: &str) -> Option<&'a Value> {
    m.get(key).or_else(|| {
        m.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    })
}

pub(super) fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Bool(_) => "boolean",
        Value::Int(_) | Value::Float(_) => "number",
        Value::Date(_) => "date",
        Value::String(s) if looks_like_date(s) => "date",
        Value::Null | Value::String(_) => "string",
        Value::Array(items) if !items.is_empty() && items.iter().all(|i| i.as_map().is_some()) => {
            "objects"
        }
        Value::Array(_) => "list",
        Value::Map(_) => "map",
    }
}

/// `YYYY-MM-DD…`.
pub(super) fn looks_like_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
}

pub(super) fn section_of(key: &str, kind: &str) -> String {
    match kind {
        "home" => String::new(),
        _ => match key.split_once('/') {
            Some((first, _)) => first.to_owned(),
            None => String::new(),
        },
    }
}

pub(super) fn sections(entries: &BTreeMap<String, Entry>) -> Vec<Section> {
    let mut keys: BTreeSet<&str> = entries.values().map(|e| e.section.as_str()).collect();
    keys.insert("");
    let mut out = Vec::new();
    for key in keys {
        let pages: Vec<&Entry> = entries
            .values()
            .filter(|e| e.section == key && e.kind == "page")
            .collect();
        let index_key = if key.is_empty() {
            "_index".to_owned()
        } else {
            format!("{key}/_index")
        };
        if pages.is_empty() && !entries.contains_key(&index_key) {
            continue;
        }
        let title = entries
            .get(&index_key)
            .filter(|e| !key.is_empty() && !e.title.is_empty() && e.title != "_index")
            .map_or_else(|| default_section_title(key), |e| e.title.clone());
        let files: Vec<&File> = pages.iter().flat_map(|e| e.files.iter()).collect();
        let bundles = pages.iter().filter(|e| e.bundle).count();
        let suffixed = files.iter().filter(|f| f.suffixed).count();
        let mut formats: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut exts: BTreeMap<String, usize> = BTreeMap::new();
        let mut seen: BTreeMap<String, &'static str> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        for f in &files {
            if f.format != "none" && f.format != "org" {
                *formats.entry(f.format).or_default() += 1;
            }
            if let Some((_, ext)) = f.path.rsplit_once('.') {
                *exts.entry(ext.to_owned()).or_default() += 1;
            }
            for (k, kind) in &f.keys {
                if !seen.contains_key(k) {
                    order.push(k.clone());
                }
                let slot = seen.entry(k.clone()).or_insert(kind);
                if *slot == "string" && *kind != "string" {
                    *slot = kind;
                }
            }
        }
        out.push(Section {
            title,
            count: pages.len(),
            style: Style {
                bundle: bundles * 2 > pages.len(),
                lang_suffix: suffixed * 2 > files.len(),
                format: top(&formats).unwrap_or("yaml"),
                ext: top(&exts).unwrap_or_else(|| "md".to_owned()),
            },
            keys: order
                .into_iter()
                .map(|k| KeyKind {
                    kind: seen[&k],
                    key: k,
                })
                .collect(),
            key: key.to_owned(),
        });
    }
    out
}

/// The key counted most often (the first of equals).
pub(super) fn top<K: Clone + Ord>(counts: &BTreeMap<K, usize>) -> Option<K> {
    let max = counts.values().copied().max()?;
    counts
        .iter()
        .find(|(_, n)| **n == max)
        .map(|(k, _)| k.clone())
}

/// `snack-reviews` → `Snack reviews`; the root is `Pages`.
pub(super) fn default_section_title(key: &str) -> String {
    if key.is_empty() {
        return "Pages".to_owned();
    }
    let words = key.replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}
