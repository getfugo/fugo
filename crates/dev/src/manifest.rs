//! Manifests of a site build (docs/rust-port/REWRITE_PLAN.md §7.2): one extractor for the Go
//! and the Rust output alike. The schema is documented in testdata/golden/README.md; in short,
//! per file:
//!
//! - L1: the path and, when it differs, its normalised form (`norm`): `_hu_<hex>` -> `_hu_H`,
//!   fingerprints `.<16-64 hex>.` -> `.H.`;
//! - L2: HTML: `<title>`, `<link rel=canonical|alternate>`, the set of internal
//!   href/src/srcset URLs, the alias target of a redirect page; XML: the link/loc/guid texts and
//!   href attributes; JSON: the key paths and URL leaves; `_redirects`/`_headers`/`robots.txt`:
//!   the line set. URLs are percent-decoded and NFC-normalised; internal ones are site paths
//!   (`/a/b/`, query and fragment kept) with the L1 normalisation applied to the path;
//! - L3: HTML: the visible text (sha256, length, words; the text itself with `--full-text`; a
//!   tag boundary is a space, except a `span`'s inside `pre`/`code`, so highlighter token spans
//!   do not split words) and the heading ids;
//! - L4: size and sha256 of every file, `static` for files copied from static/; images: width,
//!   height and format from the file header; CSS/JS: non-empty and referenced by an HTML page.
//!
//! The golden manifests of the Go build also record its stats file, written next to its
//! configuration, under a `project:` key ([`PROJECT_PREFIX`]); this port writes no such file,
//! and structdiff leaves those entries out. The golden manifests were written by the harness's
//! first implementation: this one reproduces its output byte for byte ([`crate::py`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::html::{self, Handler};
use crate::py::{self, Dict, Py};
use crate::{Fail, fail, url};

pub const SCHEMA: &str = "ssg-manifest/1";
pub const LEVELS: [&str; 4] = ["L1", "L2", "L3", "L4"];
/// The prefix of the golden manifests' keys for files of the project directory (the Go
/// build's stats file); extraction records none.
pub const PROJECT_PREFIX: &str = "project:";

/// The L1 normalisation of an output path: `_hu_<hex>` -> `_hu_H`, then fingerprints
/// `.<16-64 lower-case hex>` before a `.` -> `.H`.
#[must_use]
pub fn norm_path(p: &str) -> String {
    let p = replace_hu(p);
    let b = p.as_bytes();
    let mut out = String::with_capacity(p.len());
    let mut last = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'.' {
            let hex = b[i + 1..]
                .iter()
                .take_while(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
                .count();
            if (16..=64).contains(&hex) && b.get(i + 1 + hex) == Some(&b'.') {
                out.push_str(&p[last..i]);
                out.push_str(".H");
                i += 1 + hex;
                last = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&p[last..]);
    out
}

/// `_hu_[0-9a-f]+` -> `_hu_H`.
fn replace_hu(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    let mut rest = p;
    while let Some(at) = rest.find("_hu_") {
        let hex = rest[at + 4..]
            .bytes()
            .take_while(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
            .count();
        if hex == 0 {
            out.push_str(&rest[..at + 1]);
            rest = &rest[at + 1..];
            continue;
        }
        out.push_str(&rest[..at]);
        out.push_str("_hu_H");
        rest = &rest[at + 4 + hex..];
    }
    out.push_str(rest);
    out
}

/// The manifest type of a publish-dir file, by its name.
#[must_use]
pub fn kind_of(rel: &str) -> &'static str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let ext = py::splitext_ext(name).to_lowercase();
    if matches!(name, "_redirects" | "_headers" | "robots.txt") {
        return "lines";
    }
    match ext.as_str() {
        ".html" | ".htm" => "html",
        ".xml" => "xml",
        ".json" | ".webmanifest" => "json",
        ".css" => "css",
        ".js" | ".mjs" => "js",
        ".jpg" | ".jpeg" | ".png" | ".gif" | ".webp" | ".bmp" | ".tif" | ".tiff" | ".ico" => {
            "image"
        }
        _ => "other",
    }
}

/// URL normalisation: internal URLs (below a base URL, root-relative, or relative to the page)
/// become percent-decoded, NFC-normalised site paths with any query and fragment; external ones
/// are percent-decoded and NFC-normalised.
pub struct Urls {
    bases: Vec<(String, String)>,
}

fn clean(s: &str) -> String {
    url::unquote(s).nfc().collect()
}

impl Urls {
    /// # Errors
    /// A base URL that `urlsplit` rejects.
    pub fn new(bases: &[String]) -> Result<Urls, Fail> {
        let bases = bases
            .iter()
            .map(|b| {
                let u = url::urlsplit(b).map_err(Fail)?;
                let path = if u.path.ends_with('/') {
                    u.path
                } else {
                    u.path + "/"
                };
                Ok((u.netloc.to_lowercase(), path))
            })
            .collect::<Result<_, Fail>>()?;
        Ok(Urls { bases })
    }

    /// The site path of an internal URL, or `None`.
    ///
    /// # Errors
    /// A URL that `urlsplit` rejects.
    pub fn internal(&self, url: &str, page: &str) -> Result<Option<String>, Fail> {
        let url = py::strip(url);
        if url.is_empty()
            || ["mailto:", "tel:", "javascript:", "data:"]
                .iter()
                .any(|p| url.starts_with(p))
        {
            return Ok(None);
        }
        let u = url::urlsplit(url).map_err(Fail)?;
        let path = if !u.scheme.is_empty() || !u.netloc.is_empty() {
            let host = u.netloc.to_lowercase();
            let with_slash = format!("{}/", u.path);
            let Some((_, bpath)) = self
                .bases
                .iter()
                .find(|(n, b)| *n == host && with_slash.starts_with(b.as_str()))
            else {
                return Ok(None);
            };
            format!("/{}", u.path.get(bpath.len()..).unwrap_or(""))
        } else if u.path.starts_with('/') {
            let with_slash = format!("{}/", u.path);
            match self
                .bases
                .iter()
                .find(|(_, b)| b != "/" && with_slash.starts_with(b.as_str()))
            {
                Some((_, bpath)) => format!("/{}", u.path.get(bpath.len()..).unwrap_or("")),
                None => u.path.clone(),
            }
        } else if u.path.is_empty() {
            page.to_owned()
        } else {
            url::urljoin(page, &u.path).map_err(Fail)?
        };
        let mut out = norm_path(&clean(&path));
        if !u.query.is_empty() {
            out.push('?');
            out.push_str(&clean(&u.query));
        }
        if !u.fragment.is_empty() {
            out.push('#');
            out.push_str(&clean(&u.fragment));
        }
        Ok(Some(out))
    }

    /// [`Urls::internal`], else the cleaned URL.
    ///
    /// # Errors
    /// As [`Urls::internal`].
    pub fn any(&self, url: &str, page: &str) -> Result<String, Fail> {
        Ok(match self.internal(url, page)? {
            Some(i) => i,
            None => clean(py::strip(url)),
        })
    }
}

/// The site path a publish-dir file is served at (`a/index.html` -> `/a/`).
#[must_use]
pub fn page_url(rel: &str) -> String {
    if rel == "index.html" {
        "/".to_owned()
    } else if let Some(dir) = rel.strip_suffix("index.html").filter(|d| d.ends_with('/')) {
        format!("/{dir}")
    } else {
        format!("/{rel}")
    }
}

type Attrs = [(String, Option<String>)];

/// The attributes as a dict: a missing value is `""`, a repeated name keeps its last value.
fn attr_map(attrs: &Attrs) -> HashMap<&str, &str> {
    attrs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_deref().unwrap_or("")))
        .collect()
}

/// One pass over an HTML document: title, rel links, URL attributes, alias target, visible
/// text and heading ids.
#[derive(Default)]
pub struct HtmlScan {
    pub title: Vec<String>,
    pub rel_links: Vec<[String; 4]>,
    pub urls: Vec<String>,
    pub refresh: Option<String>,
    pub text: String,
    pub ids: Vec<String>,
    skip: i64,
    title_buf: Option<String>,
    code: i64,
}

impl HtmlScan {
    #[must_use]
    pub fn scan(text: &str) -> HtmlScan {
        let mut s = HtmlScan::default();
        html::parse(text, &mut s);
        s
    }

    /// A tag boundary separates words, except a `span` in code: highlighters wrap tokens in
    /// spans that touch (their span structure is an allowed difference, §7.3).
    fn boundary(&mut self, tag: &str) {
        if !(tag == "span" && self.code != 0) {
            self.text.push(' ');
        }
    }

    /// The visible text: typographic quotes, dashes and the no-break space as ASCII, `…` as
    /// `...`, whitespace collapsed.
    #[must_use]
    pub fn visible_text(&self) -> String {
        let t: String = self
            .text
            .chars()
            .map(|c| match c {
                '\u{2018}' | '\u{2019}' | '\u{201a}' => '\'',
                '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{ab}' | '\u{bb}' => '"',
                '\u{2013}' | '\u{2014}' => '-',
                '\u{a0}' => ' ',
                c => c,
            })
            .collect();
        py::collapse_ws(&t.replace('\u{2026}', "..."))
    }
}

/// `re.search(r"url=(.*)$", content, re.I)`: the text after the first `url=` that runs to the
/// end (or to a final newline).
fn refresh_url(content: &str) -> Option<&str> {
    let lower = content.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower.get(from..).and_then(|l| l.find("url=")) {
        let start = from + at + 4;
        let rest = &content[start..];
        match rest.find('\n') {
            None => return Some(rest),
            Some(p) if p == rest.len() - 1 => return Some(&rest[..p]),
            Some(_) => from = from + at + 1,
        }
    }
    None
}

impl Handler for HtmlScan {
    fn starttag(&mut self, tag: &str, attrs: &Attrs) {
        let a = attr_map(attrs);
        self.boundary(tag);
        if tag == "pre" || tag == "code" {
            self.code += 1;
        }
        if tag == "script" || tag == "style" {
            self.skip += 1;
        }
        if tag == "title" {
            self.title_buf = Some(String::new());
        }
        if tag == "link"
            && let Some(href) = a.get("href")
        {
            let rel = a.get("rel").copied().unwrap_or("").to_lowercase();
            let rels = py::split_ws(&rel);
            for name in ["canonical", "alternate"] {
                if rels.contains(&name) {
                    let get = |k: &str| a.get(k).copied().unwrap_or("").to_owned();
                    self.rel_links.push([
                        name.to_owned(),
                        (*href).to_owned(),
                        get("hreflang"),
                        get("type"),
                    ]);
                }
            }
        }
        for k in ["href", "src"] {
            if let Some(v) = a.get(k) {
                self.urls.push((*v).to_owned());
            }
        }
        if let Some(srcset) = a.get("srcset") {
            for c in srcset.split(',') {
                if let Some(first) = py::split_ws(c).first() {
                    self.urls.push((*first).to_owned());
                }
            }
        }
        if tag == "meta"
            && a.get("http-equiv").copied().unwrap_or("").to_lowercase() == "refresh"
            && let Some(u) = refresh_url(a.get("content").copied().unwrap_or(""))
        {
            self.refresh = Some(py::strip(u).trim_matches(['\'', '"']).to_owned());
        }
        let t: Vec<char> = tag.chars().collect();
        if t.len() == 2
            && t[0] == 'h'
            && ('1'..='6').contains(&t[1])
            && let Some(id) = a.get("id")
        {
            self.ids.push((*id).to_owned());
        }
    }

    fn startendtag(&mut self, tag: &str, attrs: &Attrs) {
        self.starttag(tag, attrs);
        if tag == "script" || tag == "style" {
            self.skip -= 1;
        }
        if tag == "pre" || tag == "code" {
            self.code -= 1;
        }
    }

    fn endtag(&mut self, tag: &str) {
        self.boundary(tag);
        if (tag == "pre" || tag == "code") && self.code != 0 {
            self.code -= 1;
        }
        if (tag == "script" || tag == "style") && self.skip != 0 {
            self.skip -= 1;
        }
        if tag == "title"
            && let Some(t) = self.title_buf.take()
        {
            self.title.push(py::collapse_ws(&t));
        }
    }

    fn data(&mut self, data: &str) {
        if let Some(t) = &mut self.title_buf {
            t.push_str(data);
        }
        if self.skip == 0 {
            self.text.push_str(data);
        }
    }
}

/// The link/loc/guid element texts and href attributes of a feed or sitemap, in order (the
/// HTML tokenizer rather than an XML parser: it takes any namespace prefix and never fails).
#[derive(Default)]
pub struct XmlScan {
    pub items: Vec<(String, String)>,
    el: Option<String>,
    buf: String,
}

impl Handler for XmlScan {
    fn starttag(&mut self, tag: &str, attrs: &Attrs) {
        for (k, v) in attrs {
            if k == "href"
                && let Some(v) = v.as_deref().filter(|v| !v.is_empty())
            {
                self.items.push((format!("{tag} href"), v.to_owned()));
            }
        }
        if matches!(tag, "link" | "loc" | "guid") {
            self.el = Some(tag.to_owned());
            self.buf.clear();
        }
    }

    fn endtag(&mut self, tag: &str) {
        if self.el.as_deref() == Some(tag) {
            self.items
                .push((tag.to_owned(), py::strip(&self.buf).to_owned()));
            self.el = None;
        }
    }

    fn data(&mut self, data: &str) {
        if self.el.is_some() {
            self.buf.push_str(data);
        }
    }
}

fn u16_be(b: &[u8], at: usize) -> u32 {
    u32::from(u16::from_be_bytes([b[at], b[at + 1]]))
}
fn u16_le(b: &[u8], at: usize) -> u32 {
    u32::from(u16::from_le_bytes([b[at], b[at + 1]]))
}
fn uint_le(b: &[u8]) -> u32 {
    b.iter().rev().fold(0, |acc, &x| acc << 8 | u32::from(x))
}

/// (width, height, format) from an image header.
#[must_use]
pub fn image_info(b: &[u8]) -> Option<(i64, i64, &'static str)> {
    let png = b.starts_with(b"\x89PNG\r\n\x1a\n") && b.len() >= 24;
    if png {
        let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
        let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
        return Some((w.into(), h.into(), "png"));
    }
    if (b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) && b.len() >= 10 {
        return Some((u16_le(b, 6).into(), u16_le(b, 8).into(), "gif"));
    }
    if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") && b.len() >= 30 {
        match &b[12..16] {
            b"VP8 " => {
                return Some((
                    (u16_le(b, 26) & 0x3FFF).into(),
                    (u16_le(b, 28) & 0x3FFF).into(),
                    "webp",
                ));
            }
            b"VP8L" => {
                let bits = uint_le(&b[21..25]);
                return Some((
                    ((bits & 0x3FFF) + 1).into(),
                    (((bits >> 14) & 0x3FFF) + 1).into(),
                    "webp",
                ));
            }
            b"VP8X" => {
                return Some((
                    (uint_le(&b[24..27]) + 1).into(),
                    (uint_le(&b[27..30]) + 1).into(),
                    "webp",
                ));
            }
            _ => {}
        }
    }
    if b.starts_with(b"\xff\xd8") {
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = b[i + 1];
            if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
                i += 2;
                continue;
            }
            let seg = u16_be(b, i + 2) as usize;
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                return Some((u16_be(b, i + 7).into(), u16_be(b, i + 5).into(), "jpeg"));
            }
            i += 2 + seg;
        }
    }
    if b.starts_with(b"BM") && b.len() >= 26 {
        let w = i32::from_le_bytes([b[18], b[19], b[20], b[21]]);
        let h = i32::from_le_bytes([b[22], b[23], b[24], b[25]]);
        return Some((w.into(), i64::from(h).abs(), "bmp"));
    }
    if b.starts_with(b"\x00\x00\x01\x00") && b.len() >= 8 {
        let dim = |x: u8| if x == 0 { 256 } else { i64::from(x) };
        return Some((dim(b[6]), dim(b[7]), "ico"));
    }
    None
}

/// `re.sub(r"\[\d+\]", "[]", at)`.
fn generic_indices(at: &str) -> String {
    let cs: Vec<char> = at.chars().collect();
    let mut out = String::with_capacity(at.len());
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '[' {
            let digits = cs[i + 1..]
                .iter()
                .take_while(|&&c| py::is_decimal(c))
                .count();
            if digits > 0 && cs.get(i + 1 + digits) == Some(&']') {
                out.push_str("[]");
                i += digits + 2;
                continue;
            }
        }
        out.push(cs[i]);
        i += 1;
    }
    out
}

fn json_structure(
    v: &Py,
    at: &str,
    keys: &mut BTreeSet<String>,
    found: &mut Vec<Py>,
    urls: &Urls,
    page: &str,
) -> Result<(), Fail> {
    match v {
        Py::Dict(d) => {
            let mut ks: Vec<&String> = d.keys().collect();
            ks.sort();
            for k in ks {
                json_structure(&d[k], &format!("{at}.{k}"), keys, found, urls, page)?;
            }
            if d.is_empty() {
                keys.insert(format!("{at}{{}}"));
            }
        }
        Py::List(items) => {
            for (i, x) in items.iter().enumerate() {
                json_structure(x, &format!("{at}[{i}]"), keys, found, urls, page)?;
            }
            if items.is_empty() {
                keys.insert(format!("{at}[]"));
            }
        }
        _ => {
            keys.insert(generic_indices(at));
            if let Py::Str(s) = v
                && (s.starts_with('/') || s.contains("://"))
            {
                found.push(Py::Str(format!("{at} {}", urls.any(s, page)?)));
            }
        }
    }
    Ok(())
}

/// The base URLs of a site directory's config.toml: `baseURL` and that of each language (key
/// names case-insensitive, in the file's order).
///
/// # Errors
/// An unreadable or invalid config.toml.
pub fn site_config(project: &Path) -> Result<Vec<String>, Fail> {
    let fn_ = project.join("config.toml");
    if !fn_.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&fn_).map_err(|e| fail!("{}: {e}", fn_.display()))?;
    let doc = toml::de::DeTable::parse(&text).map_err(|e| fail!("{}: {e}", fn_.display()))?;
    let as_string = |v: &toml::de::DeValue<'_>| match v {
        toml::de::DeValue::String(s) => Ok(s.to_string()),
        other => Err(fail!(
            "{}: baseURL is not a string: {other:?}",
            fn_.display()
        )),
    };
    let mut lower: IndexMap<String, &toml::de::DeValue<'_>> = IndexMap::new();
    for (k, v) in in_order(doc.get_ref()) {
        lower.insert(k.to_lowercase(), v);
    }
    let mut bases = Vec::new();
    if let Some(b) = lower.get("baseurl") {
        bases.push(as_string(b)?);
    }
    if let Some(toml::de::DeValue::Table(languages)) = lower.get("languages") {
        for (_, lang) in in_order(languages) {
            if let toml::de::DeValue::Table(lang) = lang {
                for (k, v) in in_order(lang) {
                    if k.to_lowercase() == "baseurl" {
                        bases.push(as_string(v)?);
                    }
                }
            }
        }
    }
    Ok(bases)
}

/// The entries of a TOML table in the document's order.
fn in_order<'a, 'i>(t: &'a toml::de::DeTable<'i>) -> Vec<(&'a str, &'a toml::de::DeValue<'i>)> {
    let mut entries: Vec<(usize, &str, &toml::de::DeValue<'i>)> = t
        .iter()
        .map(|(k, v)| (k.span().start, k.get_ref().as_ref(), v.get_ref()))
        .collect();
    entries.sort_by_key(|e| e.0);
    entries.into_iter().map(|(_, k, v)| (k, v)).collect()
}

/// The files below `root`, as sorted `/`-separated relative paths (symbolic links to
/// directories are not entered).
#[must_use]
pub fn walk(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            if e.file_type().is_dir() {
                return false;
            }
            !(e.file_type().is_symlink() && e.path().is_dir())
        })
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

fn sha256_hex(b: &[u8]) -> String {
    let d = Sha256::digest(b);
    d.iter().map(|x| format!("{x:02x}")).collect()
}

/// The manifest entries of every file below `publish`, by path.
///
/// # Errors
/// An unreadable file, or a URL the URL functions reject.
pub fn extract(
    publish: &Path,
    project: Option<&Path>,
    bases: &[String],
    levels: &[String],
    full_text: bool,
) -> Result<IndexMap<String, Py>, Fail> {
    let has = |l: &str| levels.iter().any(|x| x == l);
    let urls = Urls::new(bases)?;
    let static_dir = project.map(|p| p.join("static"));
    let mut files: IndexMap<String, Py> = IndexMap::new();
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for rel in walk(publish) {
        let path = publish.join(&rel);
        let b = std::fs::read(&path).map_err(|e| fail!("{}: {e}", path.display()))?;
        let kind = kind_of(&rel);
        let mut e = Dict::new();
        e.insert("type".into(), kind.into());
        let n = norm_path(&rel);
        if n != rel {
            e.insert("norm".into(), n.into());
        }
        let page = page_url(&rel);
        if kind == "html" {
            let scan = HtmlScan::scan(&String::from_utf8_lossy(&b));
            let mut links = BTreeSet::new();
            for u in &scan.urls {
                if let Some(x) = urls.internal(u, &page)? {
                    links.insert(x);
                }
            }
            for x in &links {
                let x = x.split('#').next().unwrap_or("");
                referenced.insert(x.split('?').next().unwrap_or("").to_owned());
            }
            if let Some(refresh) = &scan.refresh {
                e.insert("type".into(), "alias".into());
                if has("L2") {
                    e.insert(
                        "L2".into(),
                        crate::dict! { "alias" => urls.any(refresh, &page)? },
                    );
                }
            } else {
                if has("L2") {
                    let mut rel_links = Vec::new();
                    for [r, h, hl, t] in &scan.rel_links {
                        rel_links.push(Py::List(vec![
                            r.into(),
                            urls.any(h, &page)?.into(),
                            hl.into(),
                            t.into(),
                        ]));
                    }
                    e.insert(
                        "L2".into(),
                        crate::dict! {
                            "title" => scan.title.clone(),
                            "rel" => rel_links,
                            "links" => links.into_iter().collect::<Vec<String>>(),
                        },
                    );
                }
                if has("L3") {
                    let text = scan.visible_text();
                    let mut t = Dict::new();
                    t.insert("sha256".into(), sha256_hex(text.as_bytes()).into());
                    t.insert("len".into(), py::len(&text).into());
                    t.insert("words".into(), py::split_ws(&text).len().into());
                    if full_text {
                        t.insert("t".into(), text.into());
                    }
                    e.insert(
                        "L3".into(),
                        crate::dict! { "text" => t, "ids" => scan.ids.clone() },
                    );
                }
            }
        } else if kind == "xml" && has("L2") {
            let mut scan = XmlScan::default();
            html::parse(&String::from_utf8_lossy(&b), &mut scan);
            let mut items = Vec::new();
            for (k, v) in &scan.items {
                items.push(Py::Str(format!("{k} {}", urls.any(v, &page)?)));
            }
            e.insert("L2".into(), crate::dict! { "items" => items });
        } else if kind == "json" && has("L2") {
            let l2 = match std::str::from_utf8(&b) {
                Err(_) => crate::dict! { "error" => "not JSON: UnicodeDecodeError" },
                Ok(text) => match py::loads(text) {
                    Err(_) => crate::dict! { "error" => "not JSON: JSONDecodeError" },
                    Ok(v) => {
                        let (mut keys, mut found) = (BTreeSet::new(), Vec::new());
                        json_structure(&v, "", &mut keys, &mut found, &urls, &page)?;
                        crate::dict! { "keys" => keys.into_iter().collect::<Vec<String>>(), "urls" => found }
                    }
                },
            };
            e.insert("L2".into(), l2);
        } else if kind == "lines" && has("L2") {
            let text = String::from_utf8_lossy(&b);
            let lines: BTreeSet<String> = py::splitlines(&text)
                .into_iter()
                .filter(|l| !py::strip(l).is_empty())
                .map(py::collapse_ws)
                .collect();
            e.insert(
                "L2".into(),
                crate::dict! { "lines" => lines.into_iter().collect::<Vec<String>>() },
            );
        }
        if has("L4") {
            e.insert("size".into(), b.len().into());
            e.insert("sha256".into(), sha256_hex(&b).into());
            if static_dir.as_ref().is_some_and(|s| s.join(&rel).is_file()) {
                e.insert("static".into(), true.into());
            }
            if kind == "image" {
                let info =
                    image_info(&b).map(|(w, h, f)| Py::List(vec![w.into(), h.into(), f.into()]));
                e.insert("L4".into(), crate::dict! { "image" => info });
            } else if kind == "css" || kind == "js" {
                let non_empty = b
                    .iter()
                    .any(|c| !matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c));
                e.insert("L4".into(), crate::dict! { "nonEmpty" => non_empty });
            }
        }
        files.insert(rel, Py::Dict(e));
    }
    if has("L4") {
        for (rel, e) in &mut files {
            let e = e.as_dict_mut().expect("an entry");
            if matches!(e["type"].as_str(), Some("css" | "js")) {
                let r = referenced.contains(&format!("/{}", norm_path(rel)));
                e["L4"]
                    .as_dict_mut()
                    .expect("L4")
                    .insert("referenced".into(), r.into());
            }
        }
    }
    Ok(files)
}

/// A manifest as text: sorted keys, one file per line.
#[must_use]
pub fn dumps(doc: &Dict) -> String {
    let mut lines: Vec<String> = doc
        .iter()
        .filter(|(k, _)| *k != "files")
        .map(|(k, v)| format!("{}: {}", py::json_str(k, true), py::dumps(v, false)))
        .collect();
    let files = doc
        .get("files")
        .and_then(Py::as_dict)
        .cloned()
        .unwrap_or_default();
    let mut keys: Vec<&String> = files.keys().collect();
    keys.sort();
    let body: Vec<String> = keys
        .into_iter()
        .map(|k| {
            format!(
                "{}: {}",
                py::json_str(k, false),
                py::dumps(&files[k], false)
            )
        })
        .collect();
    lines.push(format!("\"files\": {{\n{}\n}}", body.join(",\n")));
    lines.sort();
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

/// The manifest document of an extraction.
#[must_use]
pub fn document(
    site: &str,
    pass: &str,
    levels: &[String],
    bases: &[String],
    files: IndexMap<String, Py>,
) -> Dict {
    let mut doc = Dict::new();
    doc.insert("schema".into(), SCHEMA.into());
    doc.insert("site".into(), site.into());
    doc.insert("pass".into(), pass.into());
    doc.insert("levels".into(), levels.to_vec().into());
    doc.insert("baseURLs".into(), bases.to_vec().into());
    doc.insert("count".into(), files.len().into());
    doc.insert("files".into(), Py::Dict(files));
    doc
}

/// Writes `text` to `path`, gzipped (deterministically) when the name ends in `.gz`.
///
/// # Errors
/// The file cannot be written.
pub fn write_out(path: &Path, text: &str) -> Result<(), Fail> {
    let data = if path.extension().is_some_and(|e| e == "gz") {
        gzip(text.as_bytes())
    } else {
        text.as_bytes().to_vec()
    };
    std::fs::write(path, data).map_err(|e| fail!("{}: {e}", path.display()))
}

/// Gzip with no file name and time 0, at the best compression.
#[must_use]
pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut gz = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    gz.write_all(data).expect("write to memory");
    gz.finish().expect("write to memory")
}

/// A JSON file, gunzipped when its name ends in `.gz`.
///
/// # Errors
/// An unreadable file or invalid JSON.
pub fn read_json(path: &Path) -> Result<Py, Fail> {
    let raw = std::fs::read(path).map_err(|e| fail!("{}: {e}", path.display()))?;
    let raw = if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&raw[..]), &mut out)
            .map_err(|e| fail!("{}: {e}", path.display()))?;
        out
    } else {
        raw
    };
    let text = String::from_utf8(raw).map_err(|e| fail!("{}: {e}", path.display()))?;
    py::loads(&text).map_err(|e| fail!("{}: {e}", path.display()))
}

/// One line about a manifest or a structure dump.
///
/// # Errors
/// An unreadable file.
pub fn summary(path: &Path) -> Result<String, Fail> {
    let doc = read_json(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let count = |k: &str| doc.get(k).and_then(Py::as_list).map_or(0, <[Py]>::len);
    if doc
        .get("schema")
        .and_then(Py::as_str)
        .unwrap_or("")
        .starts_with("ssg-structure/")
    {
        let records = doc.get("records").and_then(Py::as_list).unwrap_or(&[]);
        let written = records
            .iter()
            .filter(|r| r.get("written").is_none_or(Py::truthy))
            .count();
        return Ok(format!(
            "{name}: {} (page, format) records ({written} written), {} aliases, {} page/1 aliases, {} resources, {} pages, {} layout entries",
            records.len(),
            count("aliases"),
            count("pagerAliases"),
            count("resources"),
            count("pages"),
            doc.get("layouts").map_or(0, |l| l
                .as_list()
                .map_or_else(|| l.as_dict().map_or(0, IndexMap::len), <[Py]>::len)),
        ));
    }
    let files = doc
        .get("files")
        .and_then(Py::as_dict)
        .cloned()
        .unwrap_or_default();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for e in files.values() {
        *kinds.entry(py::str_of(e.get_or_none("type"))).or_default() += 1;
    }
    let public = files
        .keys()
        .filter(|k| !k.starts_with(PROJECT_PREFIX))
        .count();
    let total = doc.get("count").and_then(Py::as_i64).unwrap_or(0);
    let levels: Vec<String> = doc
        .get("levels")
        .and_then(Py::as_list)
        .unwrap_or(&[])
        .iter()
        .map(py::str_of)
        .collect();
    let kinds: Vec<String> = kinds.iter().map(|(k, v)| format!("{k} {v}")).collect();
    Ok(format!(
        "{name}: {total} files ({public} in publishDir + {} in the project dir); levels {}; {}",
        total - i64::try_from(public).unwrap_or(0),
        levels.join(","),
        kinds.join(", "),
    ))
}

/// The output of `manifest extract`.
pub struct Extract {
    pub publish: PathBuf,
    pub project: Option<PathBuf>,
    pub base_urls: Vec<String>,
    pub levels: String,
    pub site: String,
    pub pass: String,
    pub full_text: bool,
    pub out: Option<PathBuf>,
}

/// `manifest extract`.
///
/// # Errors
/// Unknown levels, or a failed extraction.
pub fn run_extract(a: &Extract) -> Result<(), Fail> {
    let levels: Vec<String> = a
        .levels
        .split(',')
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    let bad: Vec<&String> = levels
        .iter()
        .filter(|l| !LEVELS.contains(&l.as_str()))
        .collect();
    if !bad.is_empty() {
        return Err(fail!("unknown levels {bad:?}"));
    }
    let bases = if !a.base_urls.is_empty() {
        a.base_urls.clone()
    } else if let Some(p) = &a.project {
        site_config(p)?
    } else {
        Vec::new()
    };
    let files = extract(
        &a.publish,
        a.project.as_deref(),
        &bases,
        &levels,
        a.full_text,
    )?;
    let text = dumps(&document(&a.site, &a.pass, &levels, &bases, files));
    match &a.out {
        Some(out) => write_out(out, &text),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}
