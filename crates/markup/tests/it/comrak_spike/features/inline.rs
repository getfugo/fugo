//! Alerts, emoji, linkify, the typographer, raw HTML, and pages without code fences.

use super::*;

// ───────────────────────────── alerts ─────────────────────────────

pub fn alerts(c: &DocsCorpus) -> Vec<Row> {
    let mut o = go_options(GoCfg::Site);
    o.extension.alerts = true;
    let (mut alert_total, mut alert_ok, mut regular_total, mut regular_ok) = (0, 0, 0, 0);
    let (mut titled, mut signed) = (0, 0);
    for ((_, md), hooks) in c.docs.iter().zip(&c.hooks) {
        let want: Vec<String> = hooks
            .iter()
            .filter(|r| r.kind == "blockquote")
            .map(|r| {
                let f = |k: &str| r.fields.get(k).cloned().unwrap_or_default();
                titled += usize::from(!f("AlertTitle").is_empty());
                signed += usize::from(!f("AlertSign").is_empty());
                format!(
                    "{}|{}|{}|{}",
                    f("Type"),
                    f("AlertType"),
                    f("AlertTitle"),
                    f("AlertSign")
                )
            })
            .collect();
        if want.is_empty() {
            continue;
        }
        let arena = Arena::new();
        let root = parse_document(&arena, md, &o);
        let mut got = Vec::new();
        walk(root, |n| match &n.data().value {
            NodeValue::BlockQuote => got.push("regular|||".to_owned()),
            NodeValue::Alert(a) => got.push(format!(
                "alert|{}|{}|",
                a.alert_type.default_title().to_lowercase(),
                a.title.clone().unwrap_or_default()
            )),
            _ => {}
        });
        let (alerts, regular): (Vec<String>, Vec<String>) =
            want.into_iter().partition(|w| w.starts_with("alert"));
        alert_total += alerts.len();
        regular_total += regular.len();
        alert_ok += multiset_matches(&alerts, &got);
        regular_ok += multiset_matches(&regular, &got);
    }
    vec![
        Row::new(
            "GitHub alerts (type, title, sign)".into(),
            "alert blockquotes",
            alert_ok,
            alert_total,
        )
        .note(format!(
            "with title {titled}, with sign {signed}; comrak: 5 fixed types, no sign"
        )),
        Row::new(
            "regular blockquotes (alerts on)".into(),
            "blockquotes",
            regular_ok,
            regular_total,
        ),
    ]
}

// ───────────────────────────── emoji ─────────────────────────────

/// `:name:` candidates in text nodes (not code), with comrak's shortcode result.
pub fn emoji(c: &DocsCorpus, goldmark_emoji: Option<&BTreeMap<String, String>>) -> Row {
    let mut o = go_options(GoCfg::Default);
    o.extension.shortcodes = true;
    let (mut candidates, mut comrak_hits, mut agree, mut goldmark_hits) = (0, 0, 0, 0);
    let mut disagree = Vec::new();
    for (_, md) in &c.docs {
        let arena = Arena::new();
        let root = parse_document(&arena, md, &go_options(GoCfg::Default));
        let mut names = Vec::new();
        walk(root, |n| {
            if let NodeValue::Text(t) = &n.data().value {
                names.extend(shortcode_candidates(t));
            }
        });
        if names.is_empty() {
            continue;
        }
        let arena2 = Arena::new();
        let root2 = parse_document(&arena2, md, &o);
        let mut resolved: BTreeMap<String, String> = BTreeMap::new();
        walk(root2, |n| {
            if let NodeValue::ShortCode(sc) = &n.data().value {
                resolved.insert(sc.code.clone(), sc.emoji.clone());
            }
        });
        for name in names {
            candidates += 1;
            let ours = resolved.get(&name);
            comrak_hits += usize::from(ours.is_some());
            if let Some(table) = goldmark_emoji {
                let theirs = table.get(&name);
                goldmark_hits += usize::from(theirs.is_some());
                if ours == theirs {
                    agree += 1;
                } else if disagree.len() < 8 {
                    disagree.push(name);
                }
            }
        }
    }
    let row = Row::new(
        "emoji shortcodes :name:".into(),
        "candidates",
        comrak_hits,
        candidates,
    );
    match goldmark_emoji {
        Some(_) => row.note(format!(
            "goldmark-emoji resolves {goldmark_hits}; same result {agree}/{candidates}; e.g. differ {disagree:?}"
        )),
        None => row.note("goldmark-emoji table not given (FUGO_GOLDMARK_EMOJI_TSV)".into()),
    }
}

pub(super) fn shortcode_candidates(t: &str) -> Vec<String> {
    let mut v = Vec::new();
    let parts: Vec<&str> = t.split(':').collect();
    let mut i = 1;
    while i + 1 < parts.len() {
        let p = parts[i];
        if !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "_+-".contains(c))
        {
            v.push(p.to_owned());
            i += 2;
        } else {
            i += 1;
        }
    }
    v
}

// ───────────────────────────── linkify, typographer, raw HTML ─────────────────────────────

pub fn links(c: &DocsCorpus) -> Vec<Row> {
    let o = go_options(GoCfg::Default);
    let mut rows = vec![element_row(c, GoCfg::Default, "a", "links (all <a>)", &o)];
    let (mut total, mut ok, mut extra) = (0, 0, 0);
    for ((name, md), html) in c.docs.iter().zip(&c.html) {
        let want: Vec<String> = elements(&tokens(&html[0], FOLD), "a")
            .into_iter()
            .filter(|a| is_bare(a))
            .collect();
        let got: Vec<String> = elements(&tokens(&to_html(md, &o), FOLD), "a")
            .into_iter()
            .filter(|a| is_bare(a))
            .collect();
        show("linkify", name, &want, &got);
        total += want.len();
        let m = multiset_matches(&want, &got);
        ok += m;
        extra += got.len() - m;
    }
    rows.push(
        Row::new(
            "linkify (bare URL/www/email anchors)".into(),
            "anchors",
            ok,
            total,
        )
        .note(format!("comrak-only bare anchors: {extra}")),
    );
    rows
}

/// `<a href="X">X'</a>` where X' is X minus a scheme/`mailto:` (an autolink or linkify result).
pub(super) fn is_bare(a: &str) -> bool {
    let Some(rest) = a.strip_prefix("<a href=\"") else {
        return false;
    };
    let Some((href, tail)) = rest.split_once("\">") else {
        return false;
    };
    let Some(text) = tail.strip_suffix("</a>") else {
        return false;
    };
    let href = href.replace("&quot;", "\"");
    !text.contains('<')
        && (href == text
            || href.strip_prefix("https://") == Some(text)
            || href.strip_prefix("http://") == Some(text)
            || href.strip_prefix("mailto:") == Some(text))
}

pub(super) const SMART: &[char] = &['‘', '’', '“', '”', '–', '—', '…', '«', '»'];

pub fn typographer(c: &DocsCorpus) -> Row {
    let o = go_options(GoCfg::Default);
    let (mut pages, mut pages_ok, mut chars, mut chars_ok) = (0, 0, 0, 0);
    let seq = |html: &str| -> Vec<String> {
        tokens(html, FOLD)
            .into_iter()
            .filter_map(|t| match t {
                Tok::Text(s) => Some(s),
                _ => None,
            })
            .flat_map(|s| {
                s.chars()
                    .filter(|c| SMART.contains(c))
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    for ((_, md), html) in c.docs.iter().zip(&c.html) {
        let want = seq(&html[0]);
        if want.is_empty() {
            continue;
        }
        let got = seq(&to_html(md, &o));
        pages += 1;
        pages_ok += usize::from(want == got);
        chars += want.len();
        chars_ok += multiset_matches(&want, &got);
    }
    Row::new(
        "typographer (quotes, dashes, ellipsis)".into(),
        "substitutions",
        chars_ok,
        chars,
    )
    .note(format!(
        "pages with the identical sequence {pages_ok}/{pages}"
    ))
}

/// Markdown never emits these; anything else with a tag in unsafe mode came from raw HTML.
pub(super) const MARKDOWN_TAGS: &[&str] = &[
    "a",
    "blockquote",
    "br",
    "code",
    "dd",
    "del",
    "div",
    "dl",
    "dt",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "img",
    "input",
    "li",
    "ol",
    "p",
    "pre",
    "section",
    "strong",
    "sup",
    "table",
    "tbody",
    "td",
    "th",
    "thead",
    "tr",
    "ul",
];

pub fn raw_html(c: &DocsCorpus) -> Vec<Row> {
    let safe = go_options(GoCfg::Default);
    let open = go_options(GoCfg::Site);
    let (mut omitted, mut omitted_ok, mut raw, mut raw_ok) = (0, 0, 0, 0);
    let comments = |html: &str| -> Vec<String> {
        tokens(html, FOLD)
            .into_iter()
            .filter_map(|t| match t {
                Tok::Comment(s) => Some(s.trim().to_owned()),
                _ => None,
            })
            .collect()
    };
    let raw_tags = |html: &str| -> Vec<String> {
        tokens(html, FOLD)
            .into_iter()
            .filter_map(|t| match t {
                Tok::Open { name, tag } if !MARKDOWN_TAGS.contains(&name.as_str()) => Some(tag),
                Tok::Comment(s) => Some(s),
                _ => None,
            })
            .collect()
    };
    for ((_, md), html) in c.docs.iter().zip(&c.html) {
        let want = comments(&html[GoCfg::Default as usize]);
        if !want.is_empty() {
            omitted += want.len();
            omitted_ok += multiset_matches(&want, &comments(&to_html(md, &safe)));
        }
        let want = raw_tags(&html[GoCfg::Site as usize]);
        if !want.is_empty() {
            raw += want.len();
            raw_ok += multiset_matches(&want, &raw_tags(&to_html(md, &open)));
        }
    }
    vec![
        Row::new(
            "raw HTML omitted (unsafe=false)".into(),
            "omission comments",
            omitted_ok,
            omitted,
        ),
        Row::new(
            "raw HTML passed (unsafe=true)".into(),
            "raw tags/comments",
            raw_ok,
            raw,
        ),
    ]
}

// ───────────────────────────── codeFences = false ─────────────────────────────

pub fn plain_fences(c: &DocsCorpus) -> Row {
    let o = go_options(GoCfg::Cjk);
    let pres = |html: &str| -> Vec<String> {
        let mut v = Vec::new();
        let mut rest = html;
        while let Some(s) = rest.find("<pre") {
            let e = rest[s..].find("</pre>").map_or(rest.len(), |e| s + e + 6);
            v.push(rest[s..e].to_owned());
            rest = &rest[e..];
        }
        v
    };
    let (mut total, mut ok) = (0, 0);
    for ((_, md), html) in c.docs.iter().zip(&c.html) {
        let want = pres(&html[GoCfg::Cjk as usize]);
        if want.is_empty() {
            continue;
        }
        total += want.len();
        ok += multiset_matches(&want, &pres(&to_html(md, &o)));
    }
    Row::new(
        "codeFences = false (plain <pre><code>), byte-exact".into(),
        "<pre> blocks",
        ok,
        total,
    )
}
