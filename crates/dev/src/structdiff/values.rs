//! Per-file values: the changes between sets and multisets, links and their resolution, text hunks.

use super::*;

/// The entries of a manifest by normalised path (in the order of their paths).
pub(super) fn by_norm(m: &Manifest) -> BTreeMap<String, Vec<&Entry>> {
    let mut groups: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
    for (rel, e) in &m.files {
        groups.entry(norm_path(rel)).or_default().push(e);
    }
    groups
}

pub(super) fn sorted<T: Ord>(mut v: Vec<T>) -> Vec<T> {
    v.sort();
    v
}

/// `{a, b, …}`: at most `n` items and the number of the others.
pub(super) fn short<T: std::fmt::Display>(items: &[T], n: usize) -> String {
    let shown: Vec<String> = items.iter().take(n).map(ToString::to_string).collect();
    let more = if items.len() > n {
        format!(", … {} more", items.len() - n)
    } else {
        String::new()
    };
    format!("{{{}{more}}}", shown.join(", "))
}

/// The items of `a` that are not in `b`, and those of `b` not in `a`.
pub(super) fn set_changes<T: Ord + Clone>(a: &[T], b: &[T]) -> (Vec<T>, Vec<T>) {
    let (sa, sb): (BTreeSet<&T>, BTreeSet<&T>) = (a.iter().collect(), b.iter().collect());
    (
        sa.difference(&sb).map(|x| (*x).clone()).collect(),
        sb.difference(&sa).map(|x| (*x).clone()).collect(),
    )
}

/// The items of `a` beyond their count in `b`, and those of `b` beyond `a`.
pub(super) fn multiset_changes<T: Ord + Clone>(a: &[T], b: &[T]) -> (Vec<T>, Vec<T>) {
    let count = |xs: &[T]| {
        let mut m: BTreeMap<T, usize> = BTreeMap::new();
        for x in xs {
            *m.entry(x.clone()).or_default() += 1;
        }
        m
    };
    let (ca, cb) = (count(a), count(b));
    let extra = |x: &BTreeMap<T, usize>, y: &BTreeMap<T, usize>| -> Vec<T> {
        x.iter()
            .flat_map(|(k, n)| {
                std::iter::repeat_n(k.clone(), n.saturating_sub(y.get(k).copied().unwrap_or(0)))
            })
            .collect()
    };
    (extra(&ca, &cb), extra(&cb, &ca))
}

pub(super) fn changes<T: std::fmt::Display>(what: &str, gone: &[T], new: &[T]) -> String {
    format!("{what} -{} +{}", short(gone, 6), short(new, 6))
}

/// The classes and detail of two different L2 values of one file.
pub(super) fn l2_diff(r: (Kind, &L2), c: (Kind, &L2)) -> (Vec<String>, String) {
    if r.0 != c.0 {
        return (
            vec!["L2 type".into()],
            format!("type {} -> {}", r.0.name(), c.0.name()),
        );
    }
    let (mut classes, mut parts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    match (r.1, c.1) {
        (
            L2::Html {
                title: rt,
                rel: rr,
                links: rl,
            },
            L2::Html {
                title: ct,
                rel: cr,
                links: cl,
            },
        ) => {
            if rt != ct {
                classes.push("L2 html title".into());
                parts.push(format!("title {rt:?} -> {ct:?}"));
            }
            if rr != cr {
                classes.push("L2 html rel".into());
                let show =
                    |v: &[[String; 4]]| v.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>();
                let (gone, new) = set_changes(&show(rr), &show(cr));
                parts.push(if gone.is_empty() && new.is_empty() {
                    "rel order".into()
                } else {
                    changes("rel", &gone, &new)
                });
            }
            if rl != cl {
                classes.push("L2 html links".into());
                let (gone, new) = set_changes(rl, cl);
                parts.push(changes("links", &gone, &new));
            }
        }
        (L2::Alias { alias: ra }, L2::Alias { alias: ca }) => {
            classes.push("L2 alias target".into());
            parts.push(format!("alias {ra} -> {ca}"));
        }
        (L2::Xml { items: ri }, L2::Xml { items: ci }) => {
            classes.push("L2 xml items".into());
            if sorted(ri.clone()) == sorted(ci.clone()) {
                parts.push("items in another order".into());
            } else {
                let (gone, new) = multiset_changes(ri, ci);
                parts.push(changes("items", &gone, &new));
            }
        }
        (L2::Json { keys: rk, urls: ru }, L2::Json { keys: ck, urls: cu }) => {
            if rk != ck {
                classes.push("L2 json keys".into());
                let (gone, new) = set_changes(rk, ck);
                parts.push(changes("keys", &gone, &new));
            }
            if ru != cu {
                classes.push("L2 json urls".into());
                let (gone, new) = multiset_changes(ru, cu);
                parts.push(changes("urls", &gone, &new));
            }
        }
        (L2::Lines { lines: rl }, L2::Lines { lines: cl }) => {
            classes.push("L2 lines".into());
            let (gone, new) = set_changes(rl, cl);
            parts.push(changes("lines", &gone, &new));
        }
        (rv, cv) => {
            classes.push(format!("L2 {}", r.0.name()));
            parts.push(format!(
                "{} -> {}",
                crate::json::line(rv),
                crate::json::line(cv)
            ));
        }
    }
    (classes, parts.join("; "))
}

/// The detail of two groups of values (files that share a normalised path).
pub(super) fn group_detail<T: Serialize>(r: &[T], c: &[T]) -> String {
    let canon = |xs: &[T]| xs.iter().map(crate::json::line).collect::<Vec<_>>();
    let (gone, new) = multiset_changes(&canon(r), &canon(c));
    format!(
        "{} vs {} files: -{} +{}",
        r.len(),
        c.len(),
        short(&gone, 3),
        short(&new, 3)
    )
}

/// Whether an internal site path resolves to one of `files` (normalised publish paths).
#[must_use]
pub fn resolves(link: &str, files: &HashSet<String>) -> bool {
    let path = link.split(['#', '?']).next().unwrap_or("");
    let Some(rel) = path.strip_prefix('/') else {
        return true;
    };
    if rel.is_empty() {
        files.contains("index.html")
    } else if rel.ends_with('/') {
        files.contains(&format!("{rel}index.html"))
    } else {
        files.contains(rel) || files.contains(&format!("{rel}/index.html"))
    }
}

/// The internal links (and alias target) of an entry.
#[must_use]
pub fn links_of(e: &Entry) -> Vec<&String> {
    match &e.l2 {
        Some(L2::Html { links, .. }) => links.iter().collect(),
        Some(L2::Alias { alias }) => vec![alias],
        _ => Vec::new(),
    }
}

/// The internal links (and alias target) of an entry that resolve to none of `files`.
pub(super) fn dangling(e: &Entry, files: &HashSet<String>) -> BTreeSet<String> {
    links_of(e)
        .into_iter()
        .filter(|x| x.starts_with('/') && !resolves(x, files))
        .cloned()
        .collect()
}
