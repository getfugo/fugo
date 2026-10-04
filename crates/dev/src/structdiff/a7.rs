//! A7: the share of the reference's pages whose visible text the candidate has, and the worst
//! pages.

use super::*;

/// The Dice coefficient of the word multisets of two texts.
#[must_use]
pub fn words_similarity(a: &str, b: &str) -> f64 {
    pub(super) fn count(s: &str) -> HashMap<&str, usize> {
        let mut m: HashMap<&str, usize> = HashMap::new();
        for w in s.split_whitespace() {
            *m.entry(w).or_default() += 1;
        }
        m
    }
    let (ca, cb) = (count(a), count(b));
    let total: usize = ca.values().sum::<usize>() + cb.values().sum::<usize>();
    if total == 0 {
        return 1.0;
    }
    let common: usize = ca
        .iter()
        .map(|(w, n)| (*n).min(cb.get(w).copied().unwrap_or(0)))
        .sum();
    ratio(2 * common, total)
}

#[allow(clippy::cast_precision_loss)]
pub(super) fn ratio(a: usize, b: usize) -> f64 {
    a as f64 / b as f64
}

pub(super) fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// The differing stretches of two texts, word by word (`[-removed-] [+added+]`, each side cut
/// to `width` words), at most `limit`.
#[must_use]
pub fn text_hunks(a: &str, b: &str, width: usize, limit: usize) -> Vec<String> {
    let (wa, wb): (Vec<&str>, Vec<&str>) = (
        a.split_whitespace().collect(),
        b.split_whitespace().collect(),
    );
    let cut = |ws: &[&str]| {
        let head = ws.iter().take(width).copied().collect::<Vec<_>>().join(" ");
        if ws.len() > width {
            format!("{head} …")
        } else {
            head
        }
    };
    similar::capture_diff_slices(similar::Algorithm::Myers, &wa, &wb)
        .iter()
        .filter_map(|op| match op.as_tag_tuple() {
            (similar::DiffTag::Equal, ..) => None,
            (_, ra, rb) => Some(format!("[-{}-] [+{}+]", cut(&wa[ra]), cut(&wb[rb]))),
        })
        .take(limit)
        .collect()
}

/// A7, and the first hunk (and the number of hunks) of each page with differing text.
pub(super) fn a7(r: &Manifest, c: &Manifest) -> (A7, BTreeMap<String, (String, usize)>) {
    let (rg, cg) = (by_norm(r), by_norm(c));
    pub(super) fn page_text<'e>(e: &&'e Entry) -> Option<&'e manifest::Text> {
        if e.kind == Kind::Html {
            e.l3.as_ref().map(|l| &l.text)
        } else {
            None
        }
    }
    let mut pages = Vec::new();
    let mut hunks: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (n, items) in &rg {
        let Some(rt) = items.first().and_then(page_text) else {
            continue;
        };
        let score = |ratio: f64, method| PageScore {
            file: n.clone(),
            ratio,
            method,
            hunks: Vec::new(),
            hunk_count: None,
        };
        let Some(ct) = cg.get(n).and_then(|cs| cs.iter().find_map(page_text)) else {
            pages.push(score(0.0, "missing"));
            continue;
        };
        if rt.sha256 == ct.sha256 {
            pages.push(score(1.0, "equal"));
        } else if let (Some(ta), Some(tb)) = (&rt.t, &ct.t) {
            let h = text_hunks(ta, tb, 8, 20);
            for x in h.iter().collect::<BTreeSet<_>>() {
                hunks.entry(x.clone()).or_default().push(n.clone());
            }
            pages.push(PageScore {
                hunks: h.iter().take(3).cloned().collect(),
                hunk_count: Some(h.len()),
                ..score(round4(words_similarity(ta, tb)), "words")
            });
        } else {
            let (la, lb) = (rt.len, ct.len);
            let r = if la.max(lb) == 0 {
                1.0
            } else {
                ratio(la.min(lb), la.max(lb))
            };
            pages.push(score(round4(r.min(0.9999)), "len"));
        }
    }
    let equal = pages.iter().filter(|p| p.method == "equal").count();
    let firsts = pages
        .iter()
        .filter_map(|p| {
            p.hunks
                .first()
                .map(|h| (p.file.clone(), (h.clone(), p.hunk_count.unwrap_or(0))))
        })
        .collect();
    let mut worst: Vec<PageScore> = pages
        .iter()
        .filter(|p| p.method != "equal")
        .cloned()
        .collect();
    worst.sort_by(|a, b| {
        a.ratio
            .total_cmp(&b.ratio)
            .then_with(|| a.file.cmp(&b.file))
    });
    worst.truncate(WORST);
    let mut top: Vec<(String, Vec<String>)> = hunks.into_iter().collect();
    top.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let n = pages.len();
    let a7 = A7 {
        pages: n,
        equal,
        ratio: if n == 0 { 1.0 } else { round4(ratio(equal, n)) },
        mean_similarity: if n == 0 {
            1.0
        } else {
            round4(pages.iter().map(|p| p.ratio).sum::<f64>() / ratio(n, 1))
        },
        worst,
        top_hunks: top
            .into_iter()
            .take(WORST)
            .map(|(hunk, files)| HunkCount {
                hunk,
                pages: files.len(),
                examples: files.into_iter().take(3).collect(),
            })
            .collect(),
    };
    (a7, firsts)
}
