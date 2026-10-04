//! The perturbations: each changes the copy `d` and returns (description, {(key, level): class})

use super::*;

pub(super) type Want = BTreeMap<Key, String>;
pub(super) type Perturbation = fn(&Path, &Run, &Manifest) -> Result<(String, Want), Fail>;

pub(super) fn want(items: &[(&str, Level, &str)]) -> Want {
    items
        .iter()
        .map(|(k, l, c)| (((*k).to_owned(), *l), (*c).to_owned()))
        .collect()
}

/// The files with a link (or alias target) that only `rel` resolves.
pub(super) fn linking(man: &Manifest, rel: &str) -> Vec<String> {
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
pub(super) fn drop_page(
    d: &Path,
    man: &Manifest,
    most_linked: bool,
) -> Result<(String, Want), Fail> {
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
pub(super) fn drop_file(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
    drop_page(d, man, false)
}

/// The most linked page: link integrity (L2) in every file that links to it.
pub(super) fn drop_linked_page(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
    drop_page(d, man, true)
}

pub(super) fn add_file(d: &Path, _: &Run, _: &Manifest) -> Result<(String, Want), Fail> {
    let rel = "selftest-added/index.html";
    write(
        &d.join(rel),
        b"<!DOCTYPE html><html><head><title>Added</title></head><body><p>An added page</p><a href=\"/\">Home</a></body></html>\n",
    )?;
    Ok((format!("add {rel}"), want(&[(rel, Level::L1, "L1 extra")])))
}

/// `<a … href="…">` or `'…'` (case-insensitive): the text up to the value, then the value in
/// group 2 (double quotes) or 3 (single quotes).
pub(super) fn a_href_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r#"(?i)(<a\b[^>]*?\shref=)(?:"([^"']*)"|'([^"']*)')"#)
}

/// The quote and the value of an `a_href_re` or `href` match.
pub(super) fn quoted(m: &Captures<'_>) -> (char, String) {
    match m.get(2) {
        Some(v) => ('"', v.as_str().to_owned()),
        None => ('\'', m.get(3).map_or("", |v| v.as_str()).to_owned()),
    }
}

/// Every `<a href>` of one internal URL on a page, pointed at another existing page.
pub(super) fn change_link(d: &Path, _: &Run, man: &Manifest) -> Result<(String, Want), Fail> {
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
pub(super) fn reorder_tags(text: &str) -> (String, usize) {
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

pub(super) fn reorder_attributes(d: &Path, _: &Run, _: &Manifest) -> Result<(String, Want), Fail> {
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
