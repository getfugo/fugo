//! Self-test of structdiff (docs/rust-port/REWRITE_PLAN.md §7.2): synthetic perturbations of a
//! Go build's output, each compared with the unperturbed output, must be classified exactly as
//! expected (the right file, level and difference class, and nothing else):
//!
//! 1. drop a file: L1 missing
//! 2. add a file: L1 extra
//! 3. change an internal link: L2 html links
//! 4. reorder attributes: ignored (no difference)
//! 5. percent-encode a Thai href: ignored (no difference; a percent-encoded one is decoded)
//! 6. split code into token spans: ignored (no difference: a highlighter's span structure)
//! 7. change visible text: L3 text
//! 8. change image dimensions: L4 image
//! 9. change an RSS item link: L2 xml items
//! 10. drop the most linked page: L1 missing, and L2 dangling links in every file that links to
//!     it (link integrity)
//!
//! then the ratchet: against a baseline of the unperturbed output, perturbation 7 unlisted
//! fails; listed in a changes file it passes, and `--update` writes it into the baseline; the
//! unperturbed output against that baseline is an unlisted improvement, which does not fail.
//!
//! The Go output is a publish directory (with its site directory) or, by default, Go's testsite
//! output (crates/build/tests/it/testsite-go.txtar) with a Thai page and a PNG added (the
//! testsite has neither). Everything happens in a temporary directory.

use std::collections::{BTreeMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::{Captures, Regex};

use crate::manifest::{self, Urls, norm_path, page_url};
use crate::py::{self, Py};
use crate::structdiff::{self as sd, Side};
use crate::{Fail, fail, txtar, url};

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a valid expression"))
}

// ---------------------------------------------------------------------------------------------
// The Go output

fn write(path: &Path, text: &[u8]) -> Result<(), Fail> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| fail!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| fail!("{}: {e}", path.display()))
}

fn read(root: &Path, rel: &str) -> Result<String, Fail> {
    let p = root.join(rel);
    std::fs::read_to_string(&p).map_err(|e| fail!("{}: {e}", p.display()))
}

/// A valid RGB PNG of the given size.
#[must_use]
pub fn png(width: u32, height: u32) -> Vec<u8> {
    fn chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(data.len())
            .expect("a small chunk")
            .to_be_bytes()
            .to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc = crc32fast::Hasher::new();
        crc.update(kind);
        crc.update(data);
        out.extend_from_slice(&crc.finalize().to_be_bytes());
        out
    }
    // Each row: filter type 0, then the pixels.
    let row: Vec<u8> = std::iter::once(0)
        .chain(b"\x80\x40\x20".repeat(width as usize))
        .collect();
    let raw = row.repeat(height as usize);
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&raw).expect("write to memory");
    let idat = z.finish().expect("write to memory");
    let mut ihdr = width.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    out.extend(chunk(b"IHDR", &ihdr));
    out.extend(chunk(b"IDAT", &idat));
    out.extend(chunk(b"IEND", b""));
    out
}

/// Go's testsite output plus a Thai page and an image (what the testsite lacks).
fn testsite_output(dest: &Path) -> Result<(), Fail> {
    let archive = crate::root().join("crates/build/tests/it/testsite-go.txtar");
    let text =
        std::fs::read_to_string(&archive).map_err(|e| fail!("{}: {e}", archive.display()))?;
    for (name, content) in txtar::parse(&text) {
        write(&dest.join(name), content.as_bytes())?;
    }
    let th = "/th/%E0%B8%82%E0%B8%99%E0%B8%A1/";
    write(
        &dest.join("th/ขนม/index.html"),
        format!(
            "<!DOCTYPE html><html lang=\"th\"><head><title>ขนม</title></head><body><h1 id=\"khanom\">ขนม</h1><p>หน้าขนมไทย with a picture</p><a href=\"{th}\">ขนม</a> <a href=\"/\">Home</a><img src=\"/img/selftest.png\" alt=\"\"></body></html>\n"
        )
        .as_bytes(),
    )?;
    write(&dest.join("img/selftest.png"), &png(3, 2))
}

// ---------------------------------------------------------------------------------------------
// Comparison

struct Run {
    site: String,
    project: Option<PathBuf>,
}

impl Run {
    fn side(&self, name: &str, out: &Path) -> Result<Side, Fail> {
        Ok(Side {
            name: name.to_owned(),
            min: sd::load_manifest(Some(out), self.project.as_deref(), &self.site, "minified")?,
            unmin: sd::load_manifest(Some(out), self.project.as_deref(), &self.site, "unminified")?,
            structure: None,
        })
    }

    fn compare(&self, r: &Path, c: &Path) -> Result<py::Dict, Fail> {
        let mut res = sd::compare(
            &self.site,
            &self.side("go", r)?,
            &self.side("perturbed", c)?,
            &[],
        );
        sd::summarize(&mut res);
        Ok(res)
    }
}

type Diffs = BTreeMap<(String, String), Vec<String>>;

/// {(key, level): classes} of every entry that is not ok.
fn diffs(res: &py::Dict) -> Diffs {
    let mut out = BTreeMap::new();
    for section in ["files", "structure"] {
        for (k, levels) in res[section].as_dict().into_iter().flatten() {
            for (lv, e) in levels.as_dict().into_iter().flatten() {
                if e.get("status").and_then(Py::as_str) != Some("ok") {
                    let classes = e
                        .get("classes")
                        .and_then(Py::as_list)
                        .unwrap_or(&[])
                        .iter()
                        .map(py::str_of)
                        .collect();
                    out.insert((k.clone(), lv.clone()), classes);
                }
            }
        }
    }
    out
}

fn html_files(root: &Path) -> Vec<String> {
    manifest::walk(root)
        .into_iter()
        .filter(|r| r.ends_with(".html"))
        .collect()
}

fn files(man: &Py) -> &py::Dict {
    man.get("files").and_then(Py::as_dict).expect("a manifest")
}

fn type_of(e: &Py) -> &str {
    e.get("type").and_then(Py::as_str).unwrap_or("")
}

// ---------------------------------------------------------------------------------------------
// The perturbations: each changes the copy `d` and returns (description, {(key, level): class})

type Want = BTreeMap<(String, String), String>;
type Perturbation = fn(&Path, &Run, &Py) -> Result<(String, Want), Fail>;

fn want(items: &[(&str, &str, &str)]) -> Want {
    items
        .iter()
        .map(|(k, l, c)| (((*k).to_owned(), (*l).to_owned()), (*c).to_owned()))
        .collect()
}

/// The files with a link (or alias target) that only `rel` resolves.
fn linking(man: &Py, rel: &str) -> Vec<String> {
    let all: HashSet<String> = files(man).keys().map(|r| norm_path(r)).collect();
    let mut rest = all.clone();
    rest.remove(&norm_path(rel));
    let mut out = Vec::new();
    for (r, e) in files(man) {
        if r == rel || !matches!(type_of(e), "html" | "alias") {
            continue;
        }
        let l2 = e.get("L2").cloned().unwrap_or_else(|| crate::dict! {});
        let mut links: Vec<String> = l2
            .get("links")
            .and_then(Py::as_list)
            .unwrap_or(&[])
            .iter()
            .map(py::str_of)
            .collect();
        if let Some(Py::Str(a)) = l2.get("alias") {
            links.push(a.clone());
        }
        if links
            .iter()
            .any(|x| x.starts_with('/') && sd::resolves(x, &all) && !sd::resolves(x, &rest))
        {
            out.push(r.clone());
        }
    }
    out
}

/// Drops an HTML page below the root; the files that link to it get dangling links (L2).
fn drop_page(d: &Path, man: &Py, most_linked: bool) -> Result<(String, Want), Fail> {
    let mut pages: Vec<(usize, String)> = files(man)
        .iter()
        .filter(|(r, e)| type_of(e) == "html" && r.contains('/'))
        .map(|(r, _)| (linking(man, r).len(), r.clone()))
        .collect();
    pages.sort();
    let (_, rel) = if most_linked {
        pages.last()
    } else {
        pages.first()
    }
    .ok_or_else(|| fail!("drop: no HTML page below the root"))?;
    let p = d.join(rel);
    std::fs::remove_file(&p).map_err(|e| fail!("{}: {e}", p.display()))?;
    let mut w = want(&[(rel, "L1", "L1 missing")]);
    for r in linking(man, rel) {
        w.insert((r, "L2".into()), "L2 dangling links".into());
    }
    Ok((format!("drop {rel} ({} files link to it)", w.len() - 1), w))
}

/// The least linked page (the plain L1 case).
fn drop_file(d: &Path, _: &Run, man: &Py) -> Result<(String, Want), Fail> {
    drop_page(d, man, false)
}

/// The most linked page: link integrity (L2) in every file that links to it.
fn drop_linked_page(d: &Path, _: &Run, man: &Py) -> Result<(String, Want), Fail> {
    drop_page(d, man, true)
}

fn add_file(d: &Path, _: &Run, _: &Py) -> Result<(String, Want), Fail> {
    let rel = "selftest-added/index.html";
    write(
        &d.join(rel),
        b"<!DOCTYPE html><html><head><title>Added</title></head><body><p>An added page</p><a href=\"/\">Home</a></body></html>\n",
    )?;
    Ok((format!("add {rel}"), want(&[(rel, "L1", "L1 extra")])))
}

/// `(<a\b[^>]*?\shref=)(["'])([^"']*)\2` (case-insensitive), the quote in group 2 or 4.
fn a_href_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r#"(?i)(<a\b[^>]*?\shref=)(?:"([^"']*)"|'([^"']*)')"#)
}

/// The quote and the value of an `a_href_re` or `href_re` match.
fn quoted(m: &Captures<'_>) -> (char, String) {
    match m.get(2) {
        Some(v) => ('"', v.as_str().to_owned()),
        None => ('\'', m.get(3).map_or("", |v| v.as_str()).to_owned()),
    }
}

/// Every `<a href>` of one internal URL on a page, pointed at another existing page.
fn change_link(d: &Path, _: &Run, man: &Py) -> Result<(String, Want), Fail> {
    let bases: Vec<String> = man
        .get("baseURLs")
        .and_then(Py::as_list)
        .unwrap_or(&[])
        .iter()
        .map(py::str_of)
        .collect();
    let urls = Urls::new(&bases)?;
    let pages: Vec<String> = files(man)
        .iter()
        .filter(|(_, e)| type_of(e) == "html")
        .map(|(r, _)| page_url(r))
        .filter(|p| p.is_ascii())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for rel in html_files(d) {
        let Some(e) = files(man).get(&rel).filter(|e| type_of(e) == "html") else {
            continue;
        };
        let text = read(d, &rel)?;
        let own = page_url(&rel);
        let links: HashSet<String> = e
            .get_or_none("L2")
            .get("links")
            .and_then(Py::as_list)
            .unwrap_or(&[])
            .iter()
            .map(py::str_of)
            .collect();
        for m in a_href_re().captures_iter(&text) {
            let (_, raw) = quoted(&m);
            let Some(x) = urls.internal(&raw, &own)? else {
                continue;
            };
            if x == own || raw.contains('#') || raw.contains('?') {
                continue;
            }
            // Every occurrence of the value is an <a href>: the page's link set loses it.
            let anchors = a_href_re()
                .captures_iter(&text)
                .filter(|n| quoted(n).1 == raw)
                .count();
            if text.matches(&format!("\"{raw}\"")).count()
                + text.matches(&format!("'{raw}'")).count()
                != anchors
            {
                continue;
            }
            let Some(target) = pages.iter().find(|p| !links.contains(*p) && **p != own) else {
                continue;
            };
            let new = match bases.iter().find(|b| raw.starts_with(b.as_str())) {
                Some(b) => format!("{}{target}", b.trim_end_matches('/')),
                None => target.clone(),
            };
            let out = a_href_re().replace_all(&text, |n: &Captures<'_>| {
                let (q, v) = quoted(n);
                format!("{}{q}{}{q}", &n[1], if v == raw { &new } else { &v })
            });
            write(&d.join(&rel), out.as_bytes())?;
            return Ok((
                format!("{rel}: <a href> {raw} -> {new}"),
                want(&[(&rel, "L2", "L2 html links")]),
            ));
        }
    }
    Err(fail!("change link: no page with a suitable internal link"))
}

/// Every start tag with two or more attributes, attributes reversed (comments, scripts and
/// styles untouched); returns the text and the number of tags changed.
#[must_use]
pub fn reorder_tags(text: &str) -> (String, usize) {
    static SKIP: OnceLock<Regex> = OnceLock::new();
    static TAG: OnceLock<Regex> = OnceLock::new();
    static ATTR: OnceLock<Regex> = OnceLock::new();
    let skip = re(
        &SKIP,
        r"(?si)<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>",
    );
    let tag = re(&TAG, r"<([a-zA-Z][a-zA-Z0-9-]*)(\s[^<>]*?)(\s*/?)>");
    let attr = re(
        &ATTR,
        r#"\s+([^\s"'=<>/]+)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s"'=<>`]+))?"#,
    );
    let mut count = 0;
    let mut sub = |part: &str| {
        tag.replace_all(part, |m: &Captures<'_>| {
            let attrs: Vec<&str> = attr.find_iter(&m[2]).map(|a| a.as_str()).collect();
            // Only tags whose attributes parse completely (a quoted `>` ends the tag early).
            if attrs.len() < 2 || attrs.concat() != m[2] {
                return m[0].to_owned();
            }
            count += 1;
            let reversed: String = attrs
                .iter()
                .rev()
                .map(|a| format!(" {}", a.trim()))
                .collect();
            format!("<{}{reversed}{}>", &m[1], &m[3])
        })
        .into_owned()
    };
    let mut out = String::new();
    let mut pos = 0;
    for s in skip.find_iter(text) {
        out.push_str(&sub(&text[pos..s.start()]));
        out.push_str(s.as_str());
        pos = s.end();
    }
    out.push_str(&sub(&text[pos..]));
    (out, count)
}

fn reorder_attributes(d: &Path, _: &Run, _: &Py) -> Result<(String, Want), Fail> {
    let (mut total, mut changed) = (0, 0);
    for rel in html_files(d) {
        let (text, n) = reorder_tags(&read(d, &rel)?);
        if n > 0 {
            write(&d.join(&rel), text.as_bytes())?;
            total += n;
            changed += 1;
        }
    }
    if total == 0 {
        return Err(fail!("reorder: no tag with two attributes"));
    }
    Ok((
        format!("attributes reversed in {total} tags of {changed} files"),
        Want::new(),
    ))
}

fn thai_href(d: &Path, _: &Run, _: &Py) -> Result<(String, Want), Fail> {
    static HREF: OnceLock<Regex> = OnceLock::new();
    static THAI: OnceLock<Regex> = OnceLock::new();
    let href = re(&HREF, r#"(?i)(\shref=)(?:"([^"']*)"|'([^"']*)')"#);
    let thai = re(&THAI, r"(?i)[\u{0E00}-\u{0E7F}]|%E0%B[89]%[89AB][0-9A-F]");
    let mut changed: Vec<(String, String, String)> = Vec::new();
    for rel in html_files(d) {
        let text = read(d, &rel)?;
        let new = href.replace_all(&text, |m: &Captures<'_>| {
            let (q, v) = quoted(m);
            if !thai.is_match(&v) {
                return m[0].to_owned();
            }
            let nv = if v.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
                url::quote(&v, ":/?#[]@!$&'()*+,;=%~")
            } else {
                url::unquote(&v)
            };
            changed.push((rel.clone(), v.clone(), nv.clone()));
            format!("{}{q}{nv}{q}", &m[1])
        });
        if new != text {
            write(&d.join(&rel), new.as_bytes())?;
        }
    }
    let (rel, v, nv) = changed
        .first()
        .ok_or_else(|| fail!("thai href: no Thai href"))?;
    let how = if nv.contains('%') && !v.contains('%') {
        "percent-encoded"
    } else {
        "percent-decoded (Tera's form)"
    };
    Ok((
        format!(
            "{} Thai hrefs {how}, e.g. {rel}: {v} -> {nv}",
            changed.len()
        ),
        Want::new(),
    ))
}

/// A `<p>` from `pos` with a word of 4+ letters between whitespace: (start of the word, end).
fn p_word(text: &str, pos: usize, spaced: bool) -> Option<(usize, usize)> {
    static WORD: OnceLock<Regex> = OnceLock::new();
    static TEXT: OnceLock<Regex> = OnceLock::new();
    // The lookahead `(?=\s)` of the first implementation is the trailing `\s` here (only the
    // word's group is used).
    let m = if spaced {
        re(&WORD, r"(<p\b[^>]*>)([^<]*?\s)([^\W\d_]{4,})\s").captures_at(text, pos)?
    } else {
        re(&TEXT, r"(<p\b[^>]*>)([^<]*?)(\b[^\W\d_]{4,}\b)").captures_at(text, pos)?
    };
    let w = m.get(3)?;
    Some((w.start(), w.end()))
}

/// Turns a word of a paragraph into a code element with every character in its own span, as a
/// highlighter's token spans (Chroma and syntect split differently): the visible text must not
/// change.
fn split_code(d: &Path, _: &Run, man: &Py) -> Result<(String, Want), Fail> {
    for rel in html_files(d) {
        if !files(man).get(&rel).is_some_and(|e| type_of(e) == "html") {
            continue;
        }
        let text = read(d, &rel)?;
        let body = text.to_ascii_lowercase().find("<body");
        let Some((a, b)) = p_word(&text, body.unwrap_or(0), true) else {
            continue;
        };
        let word = &text[a..b];
        let spans: String = word
            .chars()
            .map(|c| format!("<span class=\"t\">{c}</span>"))
            .collect();
        write(
            &d.join(&rel),
            format!("{}<code>{spans}</code>{}", &text[..a], &text[b..]).as_bytes(),
        )?;
        return Ok((
            format!(
                "{rel}: {} as <code> with {} spans",
                py::repr_str(word),
                py::len(word)
            ),
            Want::new(),
        ));
    }
    Err(fail!("split code: no <p> with a word between spaces"))
}

fn change_text(d: &Path, _: &Run, man: &Py) -> Result<(String, Want), Fail> {
    for rel in html_files(d) {
        if !files(man).get(&rel).is_some_and(|e| type_of(e) == "html") {
            continue;
        }
        let text = read(d, &rel)?;
        let body = text.to_ascii_lowercase().find("<body");
        let Some((a, b)) = p_word(&text, body.unwrap_or(0), false) else {
            continue;
        };
        write(
            &d.join(&rel),
            format!("{}SELFTESTWORD{}", &text[..a], &text[b..]).as_bytes(),
        )?;
        return Ok((
            format!("{rel}: {} -> 'SELFTESTWORD'", py::repr_str(&text[a..b])),
            want(&[(&rel, "L3", "L3 text")]),
        ));
    }
    Err(fail!("change text: no <p> with a word"))
}

/// The image with its width one pixel larger (header only).
#[must_use]
pub fn bump_dimensions(b: &[u8]) -> Option<Vec<u8>> {
    let mut b = b.to_vec();
    if b.starts_with(b"\x89PNG\r\n\x1a\n") && b.len() >= 20 {
        let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]) + 1;
        b[16..20].copy_from_slice(&w.to_be_bytes());
        return Some(b);
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
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]).wrapping_add(1);
                b[i + 7..i + 9].copy_from_slice(&w.to_be_bytes());
                return Some(b);
            }
            i += 2 + usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
        }
    }
    None
}

fn change_image(d: &Path, run: &Run, man: &Py) -> Result<(String, Want), Fail> {
    let mut cands: Vec<&String> = files(man)
        .iter()
        .filter(|(_, e)| type_of(e) == "image")
        .map(|(r, _)| r)
        .collect();
    // A processed or bundle image first (a static one also changes its bytes at L4).
    let is_static = |r: &str| {
        run.project
            .as_ref()
            .is_some_and(|p| p.join("static").join(r).is_file())
    };
    cands.sort_by_key(|r| (is_static(r), (*r).clone()));
    for rel in cands {
        let p = d.join(rel);
        let Some(b) =
            bump_dimensions(&std::fs::read(&p).map_err(|e| fail!("{}: {e}", p.display()))?)
        else {
            continue;
        };
        write(&p, &b)?;
        return Ok((
            format!("{rel}: width + 1 in the header"),
            want(&[(&norm_path(rel), "L4", "L4 image")]),
        ));
    }
    Err(fail!("change image: no PNG or JPEG"))
}

fn change_rss_link(d: &Path, _: &Run, man: &Py) -> Result<(String, Want), Fail> {
    static ITEM: OnceLock<Regex> = OnceLock::new();
    let item = re(&ITEM, r"(?s)(<item>.*?<link>)([^<]*)(</link>)");
    let mut feeds: Vec<&String> = files(man)
        .iter()
        .filter(|(_, e)| type_of(e) == "xml")
        .map(|(r, _)| r)
        .collect();
    feeds.sort();
    for rel in feeds {
        let text = read(d, rel)?;
        let Some(m) = item.captures(&text) else {
            continue;
        };
        let old = m.get(2).expect("a group");
        let u = url::urlsplit(old.as_str()).map_err(Fail)?;
        let new = url::urlunsplit(&url::Split {
            path: format!("/selftest-moved{}", u.path),
            ..u
        });
        write(
            &d.join(rel),
            format!("{}{new}{}", &text[..old.start()], &text[old.end()..]).as_bytes(),
        )?;
        return Ok((
            format!("{rel}: first item link {} -> {new}", old.as_str()),
            want(&[(rel, "L2", "L2 xml items")]),
        ));
    }
    Err(fail!("change RSS link: no feed with an item"))
}

const PERTURBATIONS: [(&str, Perturbation); 10] = [
    ("drop a file", drop_file),
    ("add a file", add_file),
    ("change an internal link", change_link),
    ("reorder attributes (ignored)", reorder_attributes),
    ("percent-encode a Thai href (ignored)", thai_href),
    ("split code into token spans (ignored)", split_code),
    ("change visible text", change_text),
    ("change image dimensions", change_image),
    ("change an RSS item link", change_rss_link),
    // Beyond §7.2's eight: link integrity (the span split is T66's).
    ("drop a linked page (link integrity)", drop_linked_page),
];

fn listing<V: AsRef<str>>(items: impl Iterator<Item = ((String, String), V)>, n: usize) -> String {
    let mut lines: Vec<(String, String, String)> = items
        .map(|((k, lv), c)| (lv, k, c.as_ref().to_owned()))
        .collect();
    lines.sort();
    let shown: Vec<String> = lines
        .iter()
        .take(n)
        .map(|(lv, k, c)| format!("{lv} {k}: {c}"))
        .collect();
    let more = if lines.len() > n {
        format!("; … {} more", lines.len() - n)
    } else {
        String::new()
    };
    format!("{}{more}", shown.join("; "))
}

/// Whether the differences are exactly the expected ones (each with its class).
fn check(got: &Diffs, want: &Want) -> bool {
    got.len() == want.len()
        && want
            .iter()
            .all(|(k, c)| got.get(k).is_some_and(|g| g.contains(c)))
}

// ---------------------------------------------------------------------------------------------
// The ratchet

fn ratchet_checks(
    run: &Run,
    base_dir: &Path,
    text_dir: &Path,
    tmp: &Path,
) -> Result<Vec<(String, bool, String)>, Fail> {
    let mut results = Vec::new();
    let base_res = run.compare(base_dir, base_dir)?;
    let baseline = tmp.join("baseline.json");
    let (_, doc) = sd::ratchet(&base_res, None, &mut [], &run.site);
    sd::write_baseline(&baseline, &doc)?;
    let res = run.compare(base_dir, text_dir)?;
    let d = diffs(&res);
    let ((key, level), _) = d
        .iter()
        .next()
        .filter(|_| d.len() == 1)
        .ok_or_else(|| fail!("the text change is not one difference"))?;
    let count = |r: &py::Dict, k: &str| r.get(k).and_then(Py::as_list).map_or(0, <[Py]>::len);
    let (r, _) = sd::ratchet(
        &res,
        sd::read_baseline(&baseline)?.as_ref(),
        &mut [],
        &run.site,
    );
    results.push((
        "an unlisted new diff fails".into(),
        count(&r, "unlisted") > 0 && count(&r, "listed") == 0,
        format!("{} unlisted: {level} {key}", count(&r, "unlisted")),
    ));
    let changes_dir = tmp.join("changes");
    write(
        &changes_dir.join("SELFTEST.md"),
        format!(
            "# SELFTEST\n\n- {} {level} `{key}` accepted-deviation: the self-test's text change\n",
            run.site
        )
        .as_bytes(),
    )?;
    let (mut changes, errors) = sd::load_changes(&changes_dir, &["SELFTEST".into()])?;
    let (r, doc) = sd::ratchet(
        &res,
        sd::read_baseline(&baseline)?.as_ref(),
        &mut changes,
        &run.site,
    );
    let mut ok = errors.is_empty() && count(&r, "unlisted") == 0 && count(&r, "listed") == 1;
    sd::write_baseline(&baseline, &doc)?;
    let stored = sd::read_baseline(&baseline)?.and_then(|b| {
        b.get("files")
            .and_then(|f| f.get(key))
            .and_then(|e| e.get(level))
            .cloned()
    });
    let stored = stored.unwrap_or(Py::None);
    ok = ok
        && stored.get("class").and_then(Py::as_str) == Some("accepted-deviation")
        && stored.get("task").and_then(Py::as_str) == Some("SELFTEST");
    results.push((
        "a listed diff passes and --update writes it".into(),
        ok,
        format!("baseline entry {}", py::repr(&stored)),
    ));
    let (r, _) = sd::ratchet(
        &res,
        sd::read_baseline(&baseline)?.as_ref(),
        &mut [],
        &run.site,
    );
    results.push((
        "the same diff against the updated baseline passes".into(),
        count(&r, "unlisted") + count(&r, "listed") + count(&r, "improved") == 0,
        "unchanged".into(),
    ));
    let (r, _) = sd::ratchet(
        &base_res,
        sd::read_baseline(&baseline)?.as_ref(),
        &mut [],
        &run.site,
    );
    results.push((
        "an unlisted improvement does not fail".into(),
        count(&r, "unlisted") == 0 && count(&r, "improved") == 1,
        format!("{} improved", count(&r, "improved")),
    ));
    write(
        &changes_dir.join("BAD.md"),
        format!(
            "- {0} L3 `x` fixed: no such class\n- {0} L3 `x` bug-fixed:\n",
            run.site
        )
        .as_bytes(),
    )?;
    let (_, errors) = sd::load_changes(&changes_dir, &["BAD".into()])?;
    results.push((
        "a changes entry without one triage class and a reason is an error".into(),
        errors.len() == 2,
        format!("{} errors", errors.len()),
    ));
    Ok(results)
}

// ---------------------------------------------------------------------------------------------

/// Runs the self-test, printing a line per check; the number of failed checks.
///
/// # Errors
/// A perturbation that finds nothing to perturb, or a failed read or write.
pub fn run(go_out: Option<&Path>, project: Option<&Path>, keep: bool) -> Result<usize, Fail> {
    let tmp = tempfile::Builder::new()
        .prefix("ssg-selftest.")
        .tempdir()
        .map_err(|e| fail!("a temporary directory: {e}"))?;
    let result = run_in(tmp.path(), go_out, project);
    if keep {
        println!("selftest: kept {}", tmp.keep().display());
    }
    result
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), Fail> {
    for rel in manifest::walk(from) {
        let src = from.join(&rel);
        write(
            &to.join(&rel),
            &std::fs::read(&src).map_err(|e| fail!("{}: {e}", src.display()))?,
        )?;
    }
    Ok(())
}

fn run_in(tmp: &Path, go_out: Option<&Path>, project: Option<&Path>) -> Result<usize, Fail> {
    let (src, project, label) = if let Some(out) = go_out {
        (
            out.to_owned(),
            project.map(Path::to_owned),
            "go-out".to_owned(),
        )
    } else {
        let src = tmp.join("testsite");
        testsite_output(&src)?;
        // The project directory only gives the extractor the base URL.
        let project = tmp.join("testsite-project");
        write(
            &project.join("config.toml"),
            b"baseURL = \"https://example.org/\"\n",
        )?;
        (
            src,
            Some(project),
            "testsite (Go output + Thai page + PNG)".to_owned(),
        )
    };
    let base = tmp.join("base");
    copy_dir(&src, &base)?;
    let run = Run {
        site: "selftest".into(),
        project,
    };
    println!(
        "selftest: Go output of {label}: {} files",
        manifest::walk(&base).len()
    );
    let mut failures = 0;
    let ident = diffs(&run.compare(&base, &base)?);
    println!(
        "  {}  identity: {} differences",
        if ident.is_empty() { "PASS" } else { "FAIL" },
        ident.len()
    );
    failures += usize::from(!ident.is_empty());
    let man = sd::load_manifest(Some(&base), run.project.as_deref(), &run.site, "unminified")?
        .expect("a manifest");
    let mut text_dir = None;
    for (i, (name, f)) in PERTURBATIONS.iter().enumerate() {
        let d = tmp.join(format!("p{}", i + 1));
        copy_dir(&base, &d)?;
        let (what, want) = f(&d, &run, &man)?;
        let got = diffs(&run.compare(&base, &d)?);
        let ok = check(&got, &want);
        failures += usize::from(!ok);
        println!(
            "  {}  {}. {name}: {what}",
            if ok { "PASS" } else { "FAIL" },
            i + 1
        );
        let expected = listing(want.into_iter(), 4);
        println!(
            "          expected {}",
            if expected.is_empty() {
                "no difference"
            } else {
                &expected
            }
        );
        if !ok {
            let got = listing(got.into_iter().map(|(k, c)| (k, c.join(", "))), 4);
            println!(
                "          got      {}",
                if got.is_empty() {
                    "no difference"
                } else {
                    &got
                }
            );
        }
        if *name == "change visible text" {
            text_dir = Some(d);
        }
    }
    let text_dir = text_dir.expect("the text perturbation ran");
    for (name, ok, detail) in ratchet_checks(&run, &base, &text_dir, tmp)? {
        failures += usize::from(!ok);
        println!(
            "  {}  ratchet: {name} ({detail})",
            if ok { "PASS" } else { "FAIL" }
        );
    }
    println!(
        "selftest: {}",
        if failures == 0 {
            "all checks pass".to_owned()
        } else {
            format!("{failures} FAILED")
        }
    );
    Ok(failures)
}
