//! Extracting a manifest from a publish directory: the levels of each file, the site's
//! configuration and structure.

use super::*;

/// The site path a publish-dir file is served at (`a/index.html` -> `/a/`).
#[must_use]
pub fn page_url(rel: &str) -> String {
    match rel.strip_suffix("index.html") {
        Some(dir) if dir.is_empty() || dir.ends_with('/') => format!("/{dir}"),
        _ => format!("/{rel}"),
    }
}

/// Width, height and format from an image's header.
#[must_use]
pub fn image_info(bytes: &[u8]) -> Option<ImageInfo> {
    // An ICO header: count, then the first image's width and height (0 is 256).
    if bytes.starts_with(b"\0\0\x01\0") && bytes.len() >= 8 {
        let dim = |x: u8| if x == 0 { 256 } else { i64::from(x) };
        return Some((dim(bytes[6]), dim(bytes[7]), "ico".into()));
    }
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let format = match reader.format()? {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpeg",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Bmp => "bmp",
        _ => return None,
    };
    let (w, h) = reader.into_dimensions().ok()?;
    Some((w.into(), h.into(), format.into()))
}

/// The key paths (`.a.b[]`; an empty object `{}`, an empty array `[]`) and the URL leaves
/// (`<path> <url>`) of a JSON document.
pub(super) fn json_structure(
    v: &Value,
    at: &str,
    keys: &mut BTreeSet<String>,
    urls: &mut Vec<String>,
    site: &SiteUrls,
    page: &str,
) {
    static INDEX: OnceLock<Regex> = OnceLock::new();
    match v {
        Value::Object(map) => {
            let mut names: Vec<&String> = map.keys().collect();
            names.sort();
            for k in names {
                json_structure(&map[k], &format!("{at}.{k}"), keys, urls, site, page);
            }
            if map.is_empty() {
                keys.insert(format!("{at}{{}}"));
            }
        }
        Value::Array(items) => {
            for (i, x) in items.iter().enumerate() {
                json_structure(x, &format!("{at}[{i}]"), keys, urls, site, page);
            }
            if items.is_empty() {
                keys.insert(format!("{at}[]"));
            }
        }
        leaf => {
            let index = INDEX.get_or_init(|| Regex::new(r"\[\d+\]").expect("a valid expression"));
            keys.insert(index.replace_all(at, "[]").into_owned());
            if let Value::String(s) = leaf
                && (s.starts_with('/') || s.contains("://"))
            {
                urls.push(format!("{at} {}", site.any(s, page)));
            }
        }
    }
}

/// The entries of a TOML table in the document's order.
pub(super) fn in_order<'a, 'i>(
    t: &'a toml::de::DeTable<'i>,
) -> Vec<(&'a str, &'a toml::de::DeValue<'i>)> {
    let mut entries: Vec<(usize, &str, &toml::de::DeValue<'i>)> = t
        .iter()
        .map(|(k, v)| (k.span().start, k.get_ref().as_ref(), v.get_ref()))
        .collect();
    entries.sort_by_key(|e| e.0);
    entries.into_iter().map(|(_, k, v)| (k, v)).collect()
}

/// The base URLs of a site directory's config.toml: `baseURL` and that of each language (key
/// names case-insensitive, in the file's order).
///
/// # Errors
/// An unreadable or invalid config.toml.
pub fn site_config(project: &Path) -> Result<Vec<String>, Fail> {
    let path = project.join("config.toml");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| fail!("{}: {e}", path.display()))?;
    let doc = toml::de::DeTable::parse(&text).map_err(|e| fail!("{}: {e}", path.display()))?;
    let as_string = |v: &toml::de::DeValue<'_>| match v {
        toml::de::DeValue::String(s) => Ok(s.to_string()),
        other => Err(fail!(
            "{}: baseURL is not a string: {other:?}",
            path.display()
        )),
    };
    let top: BTreeMap<String, &toml::de::DeValue<'_>> = in_order(doc.get_ref())
        .into_iter()
        .map(|(k, v)| (k.to_lowercase(), v))
        .collect();
    let mut bases = Vec::new();
    if let Some(b) = top.get("baseurl") {
        bases.push(as_string(b)?);
    }
    if let Some(toml::de::DeValue::Table(languages)) = top.get("languages") {
        for (_, lang) in in_order(languages) {
            if let toml::de::DeValue::Table(lang) = lang {
                for (k, v) in in_order(lang) {
                    if k.eq_ignore_ascii_case("baseurl") {
                        bases.push(as_string(v)?);
                    }
                }
            }
        }
    }
    Ok(bases)
}

/// The files below `root`, as sorted `/`-separated relative paths (symbolic links to
/// directories are not entered).
#[must_use]
pub fn walk(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        // Not directories, nor symbolic links to them (`is_dir` follows a link).
        .filter(|e| !e.path().is_dir())
        .map(|e| {
            let rel = e.path().strip_prefix(root).expect("below the root");
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    out.sort();
    out
}

pub(super) fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// The entry of one HTML page, and the internal links it has.
pub(super) fn html_entry(
    entry: &mut Entry,
    text: &str,
    site: &SiteUrls,
    page: &str,
    levels: Levels,
    full_text: bool,
) -> BTreeSet<String> {
    let scan = scan::html(text);
    let links: BTreeSet<String> = scan
        .urls
        .iter()
        .filter_map(|u| site.internal(u, page))
        .collect();
    if let Some(target) = &scan.refresh {
        entry.kind = Kind::Alias;
        if levels.l2 {
            entry.l2 = Some(L2::Alias {
                alias: site.any(target, page),
            });
        }
        return links;
    }
    if levels.l2 {
        let rel = scan
            .rel_links
            .iter()
            .map(|r| {
                [
                    r.rel.to_owned(),
                    site.any(&r.href, page),
                    r.hreflang.clone(),
                    r.media_type.clone(),
                ]
            })
            .collect();
        entry.l2 = Some(L2::Html {
            title: scan.title.clone(),
            rel,
            links: links.iter().cloned().collect(),
        });
    }
    if levels.l3 {
        let visible = scan.visible_text();
        entry.l3 = Some(L3 {
            text: Text {
                sha256: sha256_hex(visible.as_bytes()),
                len: visible.chars().count(),
                words: visible.split_whitespace().count(),
                t: full_text.then_some(visible),
            },
            ids: scan.ids,
        });
    }
    links
}

/// The manifest entries of every file below `publish`, by path.
///
/// # Errors
/// An unreadable file or base URL.
pub fn extract(
    publish: &Path,
    project: Option<&Path>,
    bases: &[String],
    levels: Levels,
    full_text: bool,
) -> Result<BTreeMap<String, Entry>, Fail> {
    let site = SiteUrls::new(bases)?;
    let static_dir = project.map(|p| p.join("static"));
    let mut files = BTreeMap::new();
    let mut referenced = BTreeSet::new();
    for rel in walk(publish) {
        let path = publish.join(&rel);
        let bytes = std::fs::read(&path).map_err(|e| fail!("{}: {e}", path.display()))?;
        let kind = Kind::of(&rel);
        let norm = norm_path(&rel);
        let mut entry = Entry {
            kind,
            norm: (norm != rel).then_some(norm),
            l2: None,
            l3: None,
            l4: None,
            size: None,
            sha256: None,
            is_static: false,
        };
        let page = page_url(&rel);
        let text = || String::from_utf8_lossy(&bytes).into_owned();
        match kind {
            Kind::Html => {
                for link in html_entry(&mut entry, &text(), &site, &page, levels, full_text) {
                    let path = link.split(['#', '?']).next().unwrap_or("").to_owned();
                    referenced.insert(path);
                }
            }
            Kind::Xml if levels.l2 => {
                let items = scan::xml(&text())
                    .into_iter()
                    .map(|(k, v)| format!("{k} {}", site.any(&v, &page)))
                    .collect();
                entry.l2 = Some(L2::Xml { items });
            }
            Kind::Json if levels.l2 => {
                entry.l2 = Some(match serde_json::from_slice::<Value>(&bytes) {
                    Err(e) => L2::JsonError {
                        error: format!("not JSON: {e}"),
                    },
                    Ok(v) => {
                        let (mut keys, mut urls) = (BTreeSet::new(), Vec::new());
                        json_structure(&v, "", &mut keys, &mut urls, &site, &page);
                        L2::Json {
                            keys: keys.into_iter().collect(),
                            urls,
                        }
                    }
                });
            }
            Kind::Lines if levels.l2 => {
                let lines: BTreeSet<String> = text()
                    .lines()
                    .map(collapse)
                    .filter(|l| !l.is_empty())
                    .collect();
                entry.l2 = Some(L2::Lines {
                    lines: lines.into_iter().collect(),
                });
            }
            _ => {}
        }
        if levels.l4 {
            entry.size = Some(bytes.len() as u64);
            entry.sha256 = Some(sha256_hex(&bytes));
            entry.is_static = static_dir.as_ref().is_some_and(|s| s.join(&rel).is_file());
            entry.l4 = match kind {
                Kind::Image => Some(L4::Image {
                    image: image_info(&bytes),
                }),
                Kind::Css | Kind::Js => Some(L4::Asset {
                    non_empty: !bytes.trim_ascii().is_empty(),
                    referenced: None,
                }),
                _ => None,
            };
        }
        files.insert(rel, entry);
    }
    for (rel, entry) in &mut files {
        if let Some(L4::Asset { referenced: r, .. }) = &mut entry.l4 {
            *r = Some(referenced.contains(&format!("/{}", norm_path(rel))));
        }
    }
    Ok(files)
}
