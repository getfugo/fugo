//! Bundle resources (end of phase B4): owners, names, targets and each page's `.Resources`.
//!
//! A bundle file belongs to the page at the longest key above it in any language (a file in a
//! sub-bundle belongs to the sub-bundle). The owner in the file's language names it (its path
//! below the owner) and places it (below the owner's resource directory in its primary
//! format); without a page of its language there, the file is not used, as in Go. With
//! `duplicateResourceFiles` (or on multihost sites, or for owners whose content is not
//! Markdown) every translation of the owner gets its own copy of the files it has none of.
//!
//! A page's `.Resources` are the files its key owns, each in the page's language if it has
//! that file, else (not for bundled pages) in the first language that uses it; a page with a `translationKey` adds its
//! translations' files it has none of (by normalised name). Files come first, by media main
//! type and name, then the bundled pages in the default order.

use std::collections::BTreeMap;

use ssg_base::paths::ContentKey;
use ssg_base::{LangIdx, PageId, ResourceId};
use ssg_page::Markup;

use crate::relations::{Collators, default_order};
use crate::tree::PageRole;
use crate::{BundleResource, Model};

/// The key of the page owning the bundle file at `key`: the longest key above it that is a
/// page in any language.
fn owner_key(m: &Model, key: &ContentKey) -> Option<ContentKey> {
    let mut k = key.parent();
    while let Some(cur) = k {
        if m.sites.iter().any(|s| s.tree.get(&cur).is_some()) {
            return Some(cur);
        }
        k = cur.parent();
    }
    None
}

/// Whether the owner's translations each publish their own copy of its bundle files.
fn duplicates(m: &Model, owner: PageId) -> bool {
    let p = &m.pages[owner];
    m.config.multihost
        || p.meta.markup != Markup::Markdown
        || m.config.sites[p.lang]
            .markup
            .goldmark
            .duplicate_resource_files
}

/// Names `rid` after `owner` and places it below the owner's resource directory.
fn attach(m: &mut Model, rid: ResourceId, owner: PageId) {
    let o = &m.pages[owner];
    let mut base = o.path_info.original.base.clone();
    if !base.ends_with('/') {
        base.push('/');
    }
    let target_base = o.urls.first().and_then(|u| u.paths.resources.clone());
    // Go publishes the bundle files of every page with `publishResources`, rendered or not
    // (headless bundles included; `site_render.go`).
    let publish = o.meta.build.publish_resources;
    let owner_key = o.key.clone();
    let r = &mut m.bundle_resources[rid];
    let original = &r.info.original.path;
    r.name = original
        .strip_prefix(base.as_str())
        .unwrap_or_else(|| original.trim_start_matches('/'))
        .to_owned();
    r.name_normalized = if owner_key.is_home() {
        r.key.as_str().to_owned()
    } else {
        r.key
            .as_str()
            .strip_prefix(owner_key.as_str())
            .map_or(r.key.as_str(), |s| s.trim_start_matches('/'))
            .to_owned()
    };
    r.owner = Some(owner);
    r.target_base = target_base;
    r.publish = publish && r.page.is_none();
}

/// The media main type of a file, for the order of `.Resources` (`page` for bundled pages).
fn resource_type(m: &Model, r: &BundleResource) -> String {
    if r.page.is_some() {
        return "page".to_owned();
    }
    let types = &m.config.media_types;
    if let Some(mt) = r.adapter.as_deref().and_then(|a| match &a.content {
        crate::AddedContent::Text { media_type, .. } => media_type.as_deref(),
        crate::AddedContent::Resource { media_type, .. } => Some(media_type.as_str()),
    }) && let Some(id) = types.by_type(mt)
    {
        return types.get(id).main.clone();
    }
    types
        .by_suffix(&r.info.ext)
        .map_or_else(|| "application".to_owned(), |id| types.get(id).main.clone())
}

/// Sets owners, names and targets of every bundle file, and `.Resources` of every page.
pub(crate) fn assign(m: &mut Model) {
    // Owners in the files' own languages; copies for translations that duplicate them.
    let rids: Vec<ResourceId> = m.bundle_resources.ids().collect();
    let mut owned: BTreeMap<ContentKey, Vec<ResourceId>> = BTreeMap::new();
    for rid in rids {
        let (key, lang) = {
            let r = &m.bundle_resources[rid];
            (r.key.clone(), r.lang)
        };
        let Some(okey) = owner_key(m, &key) else {
            continue;
        };
        owned.entry(okey.clone()).or_default().push(rid);
        if let Some(owner) = m.sites[lang].tree.get(&okey) {
            attach(m, rid, owner);
        }
        let langs: Vec<LangIdx> = m.config.sites.ids().collect();
        for other in langs {
            if other == lang || m.sites[other].resources.contains_key(&key) {
                continue;
            }
            let Some(owner) = m.sites[other].tree.get(&okey) else {
                continue;
            };
            if m.bundle_resources[rid].page.is_some() || !duplicates(m, owner) {
                continue;
            }
            let mut copy = m.bundle_resources[rid].clone();
            copy.lang = other;
            copy.copy_of = Some(rid);
            let cid = m.bundle_resources.push(copy);
            m.sites[other].resources.insert(key.clone(), cid);
            owned.entry(okey.clone()).or_default().push(cid);
            attach(m, cid, owner);
        }
    }

    // `.Resources`: per owned key, the page's language else the first language using it.
    let collators = Collators::new(&m.config);
    let mut lists: Vec<(PageId, Vec<ResourceId>)> = Vec::new();
    for site in &m.sites {
        for (key, id) in site.tree.iter() {
            let Some(rids) = owned.get(key) else {
                continue;
            };
            let mut by_key: BTreeMap<&ContentKey, Vec<ResourceId>> = BTreeMap::new();
            for &rid in rids {
                let r = &m.bundle_resources[rid];
                if r.owner.is_some() {
                    by_key.entry(&r.key).or_default().push(rid);
                }
            }
            let chosen: Vec<ResourceId> = by_key
                .into_values()
                .filter_map(|versions| {
                    versions
                        .iter()
                        .copied()
                        .find(|&r| m.bundle_resources[r].lang == site.lang)
                        .or_else(|| {
                            // Bundled pages are only their own language's.
                            versions
                                .iter()
                                .copied()
                                .filter(|&r| m.bundle_resources[r].page.is_none())
                                .min_by_key(|&r| m.bundle_resources[r].lang)
                        })
                })
                .collect();
            lists.push((id, chosen));
        }
    }
    let mut resources: BTreeMap<PageId, Vec<ResourceId>> = lists.into_iter().collect();

    // A translationKey shares the translations' files.
    let mut extra: Vec<(PageId, Vec<ResourceId>)> = Vec::new();
    for p in &m.pages {
        if p.role != PageRole::Standalone
            || p.meta.translation_key.as_deref().is_none_or(str::is_empty)
        {
            continue;
        }
        let own = resources.get(&p.id).cloned().unwrap_or_default();
        let mut add = Vec::new();
        for &t in &p.translations {
            if t == p.id {
                continue;
            }
            for &rid in resources.get(&t).into_iter().flatten() {
                let name = &m.bundle_resources[rid].name_normalized;
                let taken = own
                    .iter()
                    .chain(&add)
                    .any(|&o| m.bundle_resources[o].name_normalized == *name);
                if !taken {
                    add.push(rid);
                }
            }
        }
        if !add.is_empty() {
            extra.push((p.id, add));
        }
    }
    for (id, add) in extra {
        resources.entry(id).or_default().extend(add);
    }

    for (id, mut list) in resources {
        let lang = m.pages[id].lang;
        list.sort_by(|&a, &b| {
            let (ra, rb) = (&m.bundle_resources[a], &m.bundle_resources[b]);
            match (ra.page, rb.page) {
                (Some(pa), Some(pb)) => default_order(m, &collators[lang], pa, None, pb, None),
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => resource_type(m, ra)
                    .cmp(&resource_type(m, rb))
                    .then_with(|| ra.name.cmp(&rb.name)),
            }
        });
        m.pages[id].resources = list;
    }
}
