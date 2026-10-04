//! L2 and L3: the links of each file and whether they resolve, and the visible text and heading ids
//! of each page.

use super::*;

pub(super) const BASE: &str = "https://example.org/";

/// An internal URL as a site path (`/x/`), or `None` for an external one.
pub(super) fn internal(url: &str) -> Option<String> {
    let url = url.trim();
    let path = if let Some(p) = url.strip_prefix(BASE) {
        format!("/{p}")
    } else if url.starts_with('/') && !url.starts_with("//") {
        url.to_owned()
    } else {
        return None;
    };
    let path = path.split(['#', '?']).next().unwrap_or_default();
    Some(percent_decode(path))
}

/// What L2 compares of one output file.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Links {
    pub(super) title: Vec<String>,
    pub(super) rel_links: Vec<(String, String)>,
    pub(super) internal: BTreeSet<String>,
    pub(super) alias_target: Option<String>,
    pub(super) feed: Vec<String>,
    pub(super) json: Vec<String>,
}

pub(super) fn links(name: &str, bytes: &[u8]) -> Links {
    let doc = String::from_utf8_lossy(bytes);
    let mut l = Links::default();
    if name.ends_with(".html") {
        l.title = element_texts(&doc, "title");
        for t in tags(&doc) {
            if t.name == "link"
                && let (Some(rel), Some(href)) = (t.attr("rel"), t.attr("href"))
                && (rel == "canonical" || rel == "alternate")
            {
                l.rel_links.push((rel.to_owned(), percent_decode(href)));
            }
            for a in ["href", "src"] {
                if let Some(u) = t.attr(a).and_then(internal) {
                    l.internal.insert(u);
                }
            }
            if let Some(set) = t.attr("srcset") {
                for c in set.split(',') {
                    if let Some(u) = c.split_whitespace().next().and_then(internal) {
                        l.internal.insert(u);
                    }
                }
            }
            if t.name == "meta"
                && t.attr("http-equiv") == Some("refresh")
                && let Some(c) = t.attr("content")
            {
                l.alias_target = c.split_once("url=").map(|(_, u)| percent_decode(u));
            }
        }
    } else if name.ends_with(".xml") {
        for e in ["link", "loc", "guid"] {
            l.feed.extend(
                element_texts(&doc, e)
                    .into_iter()
                    .map(|u| format!("{e} {u}")),
            );
        }
        for t in tags(&doc) {
            if let Some(h) = t.attr("href") {
                l.feed
                    .push(format!("{} href {}", t.name, percent_decode(h)));
            }
        }
    } else if name.ends_with(".json") {
        let v: serde_json::Value = serde_json::from_slice(bytes).expect("JSON output parses");
        json_urls(&v, "", &mut l.json);
    }
    l
}

/// The URL leaves of a JSON document (strings that look like URLs), with their key paths.
pub(super) fn json_urls(v: &serde_json::Value, at: &str, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) if s.starts_with('/') || s.contains("://") => {
            out.push(format!("{at} {}", percent_decode(s)));
        }
        serde_json::Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                json_urls(x, &format!("{at}[{i}]"), out);
            }
        }
        serde_json::Value::Object(m) => {
            for (k, x) in m {
                json_urls(x, &format!("{at}.{k}"), out);
            }
        }
        _ => {}
    }
}

/// Whether site path `p` is one of `files` (`/x/` → `x/index.html`).
pub(super) fn resolves(p: &str, files: &BTreeMap<String, Vec<u8>>) -> bool {
    let rel = p.trim_start_matches('/');
    rel.is_empty() && files.contains_key("index.html")
        || files.contains_key(rel)
        || files.contains_key(&format!("{}/index.html", rel.trim_end_matches('/')))
}

/// Internal links of `files` that resolve to none of them.
pub(super) fn dangling(files: &BTreeMap<String, Vec<u8>>) -> BTreeSet<String> {
    files
        .iter()
        .filter(|(n, _)| n.ends_with(".html"))
        .flat_map(|(n, b)| links(n, b).internal)
        .filter(|p| !resolves(p, files))
        .collect()
}

/// Visible text: comments, `script`/`style` content and tags removed, entities decoded,
/// typographic characters mapped to ASCII, whitespace collapsed.
pub(super) fn visible_text(doc: &str) -> String {
    let mut s = String::with_capacity(doc.len());
    let mut rest = doc;
    loop {
        let Some(i) = rest.find('<') else {
            s.push_str(rest);
            break;
        };
        s.push_str(&rest[..i]);
        rest = &rest[i..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        let lower = rest.get(..7).unwrap_or(rest).to_ascii_lowercase();
        let skip_to = if lower.starts_with("<script") {
            Some("</script>")
        } else if lower.starts_with("<style") {
            Some("</style>")
        } else {
            None
        };
        if let Some(close) = skip_to {
            rest = rest
                .to_ascii_lowercase()
                .find(close)
                .map_or("", |e| &rest[e + close.len()..]);
            continue;
        }
        rest = rest.find('>').map_or("", |e| &rest[e + 1..]);
        s.push(' ');
    }
    let text = decode_entities(&s);
    let mapped: String = text
        .chars()
        .map(|c| match c {
            '‘' | '’' | '‚' => '\'',
            '“' | '”' | '„' | '«' | '»' => '"',
            '–' | '—' => '-',
            '\u{a0}' => ' ',
            c => c,
        })
        .collect::<String>()
        .replace('…', "...");
    mapped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The `id`s of `h1`–`h6`, in document order.
pub(super) fn heading_ids(doc: &str) -> Vec<String> {
    tags(doc)
        .into_iter()
        .filter(|t| {
            t.name.len() == 2
                && t.name.starts_with('h')
                && (b'1'..=b'6').contains(&t.name.as_bytes()[1])
        })
        .filter_map(|t| t.attr("id").map(str::to_owned))
        .collect()
}
