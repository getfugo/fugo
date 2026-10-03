//! Translations (phase B5): `.AllTranslations` of every page of a language tree.
//!
//! A page with a front matter `translationKey` is translated by every page (of any language)
//! with the same key; any other page by the linked pages at its path in the other languages.
//! The list is in language order: language weight, then date (newest first), link title
//! (ignoring case) and file name.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use ssg_base::PageId;
use ssg_base::paths::ContentKey;
use ssg_base::text;

use crate::Model;
use crate::tree::PageRole;

/// Go's language order of two pages.
fn language_order(m: &Model, a: PageId, b: PageId) -> Ordering {
    let (pa, pb) = (&m.pages[a], &m.pages[b]);
    let (wa, wb) = (
        m.config.sites[pa.lang].language.weight,
        m.config.sites[pb.lang].language.weight,
    );
    if wa != wb {
        return match (wa, wb) {
            (_, 0) => Ordering::Less,
            (0, _) => Ordering::Greater,
            (x, y) => x.cmp(&y),
        };
    }
    let seconds = |p: &crate::Page| {
        p.meta
            .dates
            .date
            .as_ref()
            .map_or(i64::MIN, |d| d.timestamp().as_second())
    };
    seconds(pb)
        .cmp(&seconds(pa))
        .then_with(|| compare_fold(&pa.link_title, &pb.link_title))
        .then_with(|| match (&pa.source, &pb.source) {
            (Some(x), Some(y)) => x.file.abs.cmp(&y.file.abs),
            _ => Ordering::Equal,
        })
}

/// Case-insensitive order, then byte order.
fn compare_fold(a: &str, b: &str) -> Ordering {
    text::to_lower(a)
        .cmp(&text::to_lower(b))
        .then_with(|| a.cmp(b))
}

/// Sets `.AllTranslations` of every page in a language tree.
pub(crate) fn assign(m: &mut Model) {
    let mut by_translation_key: BTreeMap<&str, Vec<PageId>> = BTreeMap::new();
    for p in &m.pages {
        if p.role == PageRole::Standalone
            && let Some(k) = p.meta.translation_key.as_deref().filter(|k| !k.is_empty())
        {
            by_translation_key.entry(k).or_default().push(p.id);
        }
    }
    let mut out: Vec<(PageId, Vec<PageId>)> = Vec::new();
    for site in &m.sites {
        for (_, id) in site.tree.iter() {
            let p = &m.pages[id];
            // Pages at the page's path (a standalone page's path is not its key: sitemaps and
            // robots.txt have no translations).
            let key = ContentKey::from_source(&p.path());
            let mut all: Vec<PageId> =
                match p.meta.translation_key.as_deref().filter(|k| !k.is_empty()) {
                    Some(k) => by_translation_key.get(k).cloned().unwrap_or_default(),
                    None => m
                        .sites
                        .iter()
                        .filter_map(|s| s.tree.get(&key))
                        .filter(|&q| m.pages[q].linked())
                        .collect(),
                };
            all.sort_by(|a, b| language_order(m, *a, *b));
            out.push((id, all));
        }
    }
    for (id, all) in out {
        m.pages[id].translations = all;
    }
}
