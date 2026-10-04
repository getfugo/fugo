//! The perturbations of URLs, text, images and feeds.

use super::*;

/// What percent-encoding a URL escapes: all but the unreserved characters and the delimiters.
pub(super) const URL_ESCAPED: &AsciiSet = &NON_ALPHANUMERIC
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

pub(super) fn is_thai(c: char) -> bool {
    ('\u{0E00}'..='\u{0E7F}').contains(&c)
}

/// Thai hrefs percent-encoded, percent-encoded ones decoded: the links must not change.
pub(super) fn thai_href(d: &Path, _: &Run, _: &Manifest) -> Result<(String, Want), Fail> {
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
pub(super) fn p_word(text: &str, pos: usize, spaced: bool) -> Option<(usize, usize)> {
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
pub(super) fn body_start(text: &str) -> usize {
    text.to_ascii_lowercase().find("<body").unwrap_or(0)
}

/// Turns a word of a paragraph into a code element with every character in its own span, as a
/// highlighter's token spans (Chroma and syntect split differently): the visible text must not
/// change.
pub(super) fn split_code(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
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

pub(super) fn change_text(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
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
pub(super) fn bump_dimensions(b: &[u8]) -> Option<Vec<u8>> {
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

pub(super) fn change_image(d: &Path, run: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
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

pub(super) fn change_rss_link(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
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
