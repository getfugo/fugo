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

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use regex::{Captures, Regex};

use crate::manifest::{self, Kind, L2, Manifest, norm_path, page_url};
use crate::ratchet::{self, Accepted, Baseline};
use crate::structdiff::{self as sd, Comparison, Level, Side, Status};
use crate::urls::SiteUrls;
use crate::{Fail, fail, json, txtar};

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

/// A PNG of the given size, one colour.
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([0x80, 0x40, 0x20]));
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("encode to memory");
    out.into_inner()
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
    fn manifest(&self, out: &Path, pass: &str) -> Result<Manifest, Fail> {
        sd::load_manifest(out, self.project.as_deref(), &self.site, pass)
    }

    fn side(&self, name: &str, out: &Path) -> Result<Side, Fail> {
        Ok(Side {
            name: name.to_owned(),
            min: Some(self.manifest(out, "minified")?),
            unmin: Some(self.manifest(out, "unminified")?),
            structure: None,
        })
    }

    fn compare(&self, r: &Path, c: &Path) -> Result<Comparison, Fail> {
        Ok(sd::compare(
            &self.site,
            &self.side("go", r)?,
            &self.side("perturbed", c)?,
            &[],
        ))
    }
}

/// A (key, level) of a comparison.
type Key = (String, Level);
type Diffs = BTreeMap<Key, Vec<String>>;

/// The classes of every (key, level) that is not ok.
fn diffs(res: &Comparison) -> Diffs {
    [&res.files, &res.structure]
        .into_iter()
        .flat_map(|section| section.iter())
        .flat_map(|(k, levels)| levels.iter().map(move |(lv, o)| ((k.clone(), *lv), o)))
        .filter(|(_, o)| o.status != Status::Ok)
        .map(|(key, o)| (key, o.classes.clone()))
        .collect()
}

fn html_files(root: &Path) -> Vec<String> {
    manifest::walk(root)
        .into_iter()
        .filter(|r| r.ends_with(".html"))
        .collect()
}

fn is_html(man: &Manifest, rel: &str) -> bool {
    man.files.get(rel).is_some_and(|e| e.kind == Kind::Html)
}

// ---------------------------------------------------------------------------------------------
// The perturbations: each changes the copy `d` and returns (description, {(key, level): class})

type Want = BTreeMap<Key, String>;
type Perturbation = fn(&Path, &Run, &Manifest) -> Result<(String, Want), Fail>;

fn want(items: &[(&str, Level, &str)]) -> Want {
    items
        .iter()
        .map(|(k, l, c)| (((*k).to_owned(), *l), (*c).to_owned()))
        .collect()
}

/// The files with a link (or alias target) that only `rel` resolves.
fn linking(man: &Manifest, rel: &str) -> Vec<String> {
    let all: HashSet<String> = man.files.keys().map(|r| norm_path(r)).collect();
    let mut rest = all.clone();
    rest.remove(&norm_path(rel));
    man.files
        .iter()
        .filter(|(r, e)| *r != rel && matches!(e.kind, Kind::Html | Kind::Alias))
        .filter(|(_, e)| {
            sd::links_of(e)
                .iter()
                .any(|x| x.starts_with('/') && sd::resolves(x, &all) && !sd::resolves(x, &rest))
        })
        .map(|(r, _)| r.clone())
        .collect()
}

/// Drops an HTML page below the root; the files that link to it get dangling links (L2).
fn drop_page(d: &Path, man: &Manifest, most_linked: bool) -> Result<(String, Want), Fail> {
    let mut pages: Vec<(usize, &String)> = man
        .files
        .iter()
        .filter(|(r, e)| e.kind == Kind::Html && r.contains('/'))
        .map(|(r, _)| (linking(man, r).len(), r))
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
    let mut w = want(&[(rel, Level::L1, "L1 missing")]);
    for r in linking(man, rel) {
        w.insert((r, Level::L2), "L2 dangling links".into());
    }
    Ok((format!("drop {rel} ({} files link to it)", w.len() - 1), w))
}

/// The least linked page (the plain L1 case).
fn drop_file(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
    drop_page(d, man, false)
}

/// The most linked page: link integrity (L2) in every file that links to it.
fn drop_linked_page(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
    drop_page(d, man, true)
}

fn add_file(d: &Path, _: &Run, _: &Manifest) -> Result<(String, Want), Fail> {
    let rel = "selftest-added/index.html";
    write(
        &d.join(rel),
        b"<!DOCTYPE html><html><head><title>Added</title></head><body><p>An added page</p><a href=\"/\">Home</a></body></html>\n",
    )?;
    Ok((format!("add {rel}"), want(&[(rel, Level::L1, "L1 extra")])))
}

/// `<a … href="…">` or `'…'` (case-insensitive): the text up to the value, then the value in
/// group 2 (double quotes) or 3 (single quotes).
fn a_href_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r#"(?i)(<a\b[^>]*?\shref=)(?:"([^"']*)"|'([^"']*)')"#)
}

/// The quote and the value of an `a_href_re` or `href` match.
fn quoted(m: &Captures<'_>) -> (char, String) {
    match m.get(2) {
        Some(v) => ('"', v.as_str().to_owned()),
        None => ('\'', m.get(3).map_or("", |v| v.as_str()).to_owned()),
    }
}

/// Every `<a href>` of one internal URL on a page, pointed at another existing page.
fn change_link(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
    let urls = SiteUrls::new(&man.base_urls)?;
    let pages: BTreeSet<String> = man
        .files
        .iter()
        .filter(|(_, e)| e.kind == Kind::Html)
        .map(|(r, _)| page_url(r))
        .filter(|p| p.is_ascii())
        .collect();
    for rel in html_files(d) {
        let Some(e) = man.files.get(&rel).filter(|e| e.kind == Kind::Html) else {
            continue;
        };
        let text = read(d, &rel)?;
        let own = page_url(&rel);
        let links: HashSet<&String> = match &e.l2 {
            Some(L2::Html { links, .. }) => links.iter().collect(),
            _ => HashSet::new(),
        };
        for m in a_href_re().captures_iter(&text) {
            let (_, raw) = quoted(&m);
            let Some(x) = urls.internal(&raw, &own) else {
                continue;
            };
            if x == own || raw.contains(['#', '?']) {
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
            let Some(target) = pages.iter().find(|p| !links.contains(p) && **p != own) else {
                continue;
            };
            let new = match man.base_urls.iter().find(|b| raw.starts_with(b.as_str())) {
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
                want(&[(&rel, Level::L2, "L2 html links")]),
            ));
        }
    }
    Err(fail!("change link: no page with a suitable internal link"))
}

/// Every start tag with two or more attributes, attributes reversed (comments, scripts and
/// styles untouched); returns the text and the number of tags changed.
fn reorder_tags(text: &str) -> (String, usize) {
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

fn reorder_attributes(d: &Path, _: &Run, _: &Manifest) -> Result<(String, Want), Fail> {
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

/// What percent-encoding a URL escapes: all but the unreserved characters and the delimiters.
const URL_ESCAPED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b':')
    .remove(b'/')
    .remove(b'?')
    .remove(b'#')
    .remove(b'[')
    .remove(b']')
    .remove(b'@')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=')
    .remove(b'%');

fn is_thai(c: char) -> bool {
    ('\u{0E00}'..='\u{0E7F}').contains(&c)
}

/// Thai hrefs percent-encoded, percent-encoded ones decoded: the links must not change.
fn thai_href(d: &Path, _: &Run, _: &Manifest) -> Result<(String, Want), Fail> {
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
            let nv = if v.chars().any(is_thai) {
                utf8_percent_encode(&v, URL_ESCAPED).to_string()
            } else {
                percent_decode_str(&v).decode_utf8_lossy().into_owned()
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

/// The first `<p>` from `pos` with a word of 4+ letters (between whitespace when `spaced`): the
/// byte range of the word.
fn p_word(text: &str, pos: usize, spaced: bool) -> Option<(usize, usize)> {
    static WORD: OnceLock<Regex> = OnceLock::new();
    static TEXT: OnceLock<Regex> = OnceLock::new();
    let m = if spaced {
        re(&WORD, r"(<p\b[^>]*>)([^<]*?\s)([^\W\d_]{4,})\s").captures_at(text, pos)?
    } else {
        re(&TEXT, r"(<p\b[^>]*>)([^<]*?)(\b[^\W\d_]{4,}\b)").captures_at(text, pos)?
    };
    let w = m.get(3)?;
    Some((w.start(), w.end()))
}

/// The byte offset of `<body` (case-insensitive), else 0.
fn body_start(text: &str) -> usize {
    text.to_ascii_lowercase().find("<body").unwrap_or(0)
}

/// Turns a word of a paragraph into a code element with every character in its own span, as a
/// highlighter's token spans (Chroma and syntect split differently): the visible text must not
/// change.
fn split_code(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
    for rel in html_files(d).into_iter().filter(|r| is_html(man, r)) {
        let text = read(d, &rel)?;
        let Some((a, b)) = p_word(&text, body_start(&text), true) else {
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
                "{rel}: {word:?} as <code> with {} spans",
                word.chars().count()
            ),
            Want::new(),
        ));
    }
    Err(fail!("split code: no <p> with a word between spaces"))
}

fn change_text(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
    for rel in html_files(d).into_iter().filter(|r| is_html(man, r)) {
        let text = read(d, &rel)?;
        let Some((a, b)) = p_word(&text, body_start(&text), false) else {
            continue;
        };
        write(
            &d.join(&rel),
            format!("{}SELFTESTWORD{}", &text[..a], &text[b..]).as_bytes(),
        )?;
        return Ok((
            format!("{rel}: {:?} -> \"SELFTESTWORD\"", &text[a..b]),
            want(&[(&rel, Level::L3, "L3 text")]),
        ));
    }
    Err(fail!("change text: no <p> with a word"))
}

/// The image with its width one pixel larger (header only).
fn bump_dimensions(b: &[u8]) -> Option<Vec<u8>> {
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
            // A start-of-frame segment: precision, height, width.
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

fn change_image(d: &Path, run: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
    let mut cands: Vec<&String> = man
        .files
        .iter()
        .filter(|(_, e)| e.kind == Kind::Image)
        .map(|(r, _)| r)
        .collect();
    // A processed or bundle image first (a static one also changes its bytes at L4).
    let is_static = |r: &str| {
        run.project
            .as_ref()
            .is_some_and(|p| p.join("static").join(r).is_file())
    };
    cands.sort_by_key(|r| (is_static(r), *r));
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
            want(&[(&norm_path(rel), Level::L4, "L4 image")]),
        ));
    }
    Err(fail!("change image: no PNG or JPEG"))
}

fn change_rss_link(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
    static ITEM: OnceLock<Regex> = OnceLock::new();
    let item = re(&ITEM, r"(?s)(<item>.*?<link>)([^<]*)(</link>)");
    for (rel, _) in man.files.iter().filter(|(_, e)| e.kind == Kind::Xml) {
        let text = read(d, rel)?;
        let Some(m) = item.captures(&text) else {
            continue;
        };
        let old = m.get(2).expect("a group");
        let mut url = url::Url::parse(old.as_str())
            .map_err(|e| fail!("{rel}: item link {}: {e}", old.as_str()))?;
        url.set_path(&format!("/selftest-moved{}", url.path()));
        write(
            &d.join(rel),
            format!("{}{url}{}", &text[..old.start()], &text[old.end()..]).as_bytes(),
        )?;
        return Ok((
            format!("{rel}: first item link {} -> {url}", old.as_str()),
            want(&[(rel, Level::L2, "L2 xml items")]),
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

/// Up to `n` of the items, as `<level> <key>: <class>`, sorted by level; "no difference" for none.
fn listing(items: impl Iterator<Item = (Key, String)>, n: usize) -> String {
    let mut lines: Vec<(Level, String, String)> = items.map(|((k, lv), c)| (lv, k, c)).collect();
    if lines.is_empty() {
        return "no difference".into();
    }
    lines.sort();
    let shown: Vec<String> = lines
        .iter()
        .take(n)
        .map(|(lv, k, c)| format!("{} {k}: {c}", lv.name()))
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

fn pass(ok: bool) -> &'static str {
    if ok { "PASS" } else { "FAIL" }
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
    let path = tmp.join("baseline.json");
    let (_, doc) = ratchet::ratchet(&base_res, None, &mut [], &run.site);
    doc.write(&path)?;
    let baseline = || Baseline::read(&path);
    let res = run.compare(base_dir, text_dir)?;
    let d = diffs(&res);
    let (key, level) = match d.keys().collect::<Vec<_>>()[..] {
        [k] => k.clone(),
        _ => return Err(fail!("the text change is not one difference")),
    };
    let (r, _) = ratchet::ratchet(&res, baseline()?.as_ref(), &mut [], &run.site);
    results.push((
        "an unlisted new diff fails".into(),
        !r.unlisted.is_empty() && r.listed.is_empty(),
        format!("{} unlisted: {} {key}", r.unlisted.len(), level.name()),
    ));

    let changes_dir = tmp.join("changes");
    let entry = format!(
        "- {} {} `{key}` accepted-deviation: the self-test's text change\n",
        run.site,
        level.name()
    );
    write(
        &changes_dir.join("SELFTEST.md"),
        format!("# SELFTEST\n\n{entry}").as_bytes(),
    )?;
    let (mut changes, errors) = ratchet::load_changes(&changes_dir, &["SELFTEST".into()])?;
    let (r, doc) = ratchet::ratchet(&res, baseline()?.as_ref(), &mut changes, &run.site);
    doc.write(&path)?;
    let stored = baseline()?.and_then(|b| b.files.get(&key)?.get(&level).cloned());
    let written = matches!(
        &stored,
        Some(Accepted::Difference { class, task, .. }) if class == "accepted-deviation" && task == "SELFTEST"
    );
    results.push((
        "a listed diff passes and --update writes it".into(),
        errors.is_empty() && r.unlisted.is_empty() && r.listed.len() == 1 && written,
        format!(
            "baseline entry {}",
            stored
                .as_ref()
                .map_or_else(|| "none".to_owned(), json::line)
        ),
    ));

    let (r, _) = ratchet::ratchet(&res, baseline()?.as_ref(), &mut [], &run.site);
    results.push((
        "the same diff against the updated baseline passes".into(),
        r.unlisted.is_empty() && r.listed.is_empty() && r.improved.is_empty(),
        "unchanged".into(),
    ));

    let (r, _) = ratchet::ratchet(&base_res, baseline()?.as_ref(), &mut [], &run.site);
    results.push((
        "an unlisted improvement does not fail".into(),
        r.unlisted.is_empty() && r.improved.len() == 1,
        format!("{} improved", r.improved.len()),
    ));

    write(
        &changes_dir.join("BAD.md"),
        format!(
            "- {0} L3 `x` fixed: no such class\n- {0} L3 `x` bug-fixed:\n",
            run.site
        )
        .as_bytes(),
    )?;
    let (_, errors) = ratchet::load_changes(&changes_dir, &["BAD".into()])?;
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
        (out.to_owned(), project.map(Path::to_owned), "go-out")
    } else {
        let src = tmp.join("testsite");
        testsite_output(&src)?;
        // The project directory only gives the extractor the base URL.
        let project = tmp.join("testsite-project");
        write(
            &project.join("config.toml"),
            b"baseURL = \"https://example.org/\"\n",
        )?;
        (src, Some(project), "testsite (Go output + Thai page + PNG)")
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
        pass(ident.is_empty()),
        ident.len()
    );
    failures += usize::from(!ident.is_empty());
    let man = run.manifest(&base, "unminified")?;
    let mut text_dir = None;
    for (i, (name, perturb)) in PERTURBATIONS.iter().enumerate() {
        let d = tmp.join(format!("p{}", i + 1));
        copy_dir(&base, &d)?;
        let (what, want) = perturb(&d, &run, &man)?;
        let got = diffs(&run.compare(&base, &d)?);
        let ok = check(&got, &want);
        failures += usize::from(!ok);
        println!("  {}  {}. {name}: {what}", pass(ok), i + 1);
        println!("          expected {}", listing(want.into_iter(), 4));
        if !ok {
            println!(
                "          got      {}",
                listing(got.into_iter().map(|(k, c)| (k, c.join(", "))), 4)
            );
        }
        if *name == "change visible text" {
            text_dir = Some(d);
        }
    }
    let text_dir = text_dir.expect("the text perturbation ran");
    for (name, ok, detail) in ratchet_checks(&run, &base, &text_dir, tmp)? {
        failures += usize::from(!ok);
        println!("  {}  ratchet: {name} ({detail})", pass(ok));
    }
    let verdict = if failures == 0 {
        "all checks pass".to_owned()
    } else {
        format!("{failures} FAILED")
    };
    println!("selftest: {verdict}");
    Ok(failures)
}
