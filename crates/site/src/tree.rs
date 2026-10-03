//! Phase B1: pages into the content tree of their language.
//!
//! [`SiteTree`] maps a language's content keys to pages; its byte order is the walk order.
//! [`place`] decides every captured file's language, key and kind (after the capture overrides),
//! drops pages of a disabled `page` kind, settles keys claimed twice (the first file wins, with a
//! warning) and finds the bundle of each content file inside a leaf bundle.

use std::collections::BTreeMap;
use std::ops::Bound;

use ssg_base::diag::Diagnostic;
use ssg_base::paths::ContentKey;
use ssg_base::{Idx, PageId, PageKind};
use ssg_config::{Config, SiteConfig};
use ssg_vfs::BundleKind;

use crate::ModelError;
use crate::capture::{Capture, CapturedPage, CapturedResource};

/// A language's content tree: content key → page. Only pages of their own (not bundled content
/// files) are in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SiteTree {
    by_key: BTreeMap<ContentKey, PageId>,
}

impl SiteTree {
    /// The page at `key`.
    #[must_use]
    pub fn get(&self, key: &ContentKey) -> Option<PageId> {
        self.by_key.get(key).copied()
    }

    /// Adds a page; `false` (and no change) when the key is taken.
    pub fn insert(&mut self, key: ContentKey, page: PageId) -> bool {
        match self.by_key.entry(key) {
            std::collections::btree_map::Entry::Occupied(_) => false,
            std::collections::btree_map::Entry::Vacant(v) => {
                v.insert(page);
                true
            }
        }
    }

    /// Removes the page at `key`.
    pub fn remove(&mut self, key: &ContentKey) -> Option<PageId> {
        self.by_key.remove(key)
    }

    /// The page whose key is the longest segment-wise prefix of `key` (`key` itself included);
    /// the home page's empty key is a prefix of every key.
    #[must_use]
    pub fn longest_prefix(&self, key: &ContentKey) -> Option<(&ContentKey, PageId)> {
        let mut k = Some(key.clone());
        while let Some(cur) = k {
            if let Some((found, id)) = self.by_key.get_key_value(&cur) {
                return Some((found, *id));
            }
            k = cur.parent();
        }
        None
    }

    /// The pages strictly below `key` (segment-wise), in key order.
    pub fn descendants(&self, key: &ContentKey) -> impl Iterator<Item = (&ContentKey, PageId)> {
        self.by_key
            .range((Bound::Excluded(key), Bound::Unbounded))
            .filter(move |(k, _)| k.starts_with_segments(key))
            .map(|(k, id)| (k, *id))
    }

    /// Every page in key (walk) order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&ContentKey, PageId)> {
        self.by_key.iter().map(|(k, id)| (k, *id))
    }

    /// The number of pages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// Whether the tree is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

/// What a page is to its language's tree.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PageRole {
    /// A page of its own, in the content tree.
    Standalone,
    /// A content file inside a leaf bundle: its content is rendered, but it has no output
    /// files. `bundle` is the key of the bundle's index page (the owner, in whichever language
    /// has one: [`crate::Model::bundle_owner`]).
    Bundled { bundle: ContentKey },
}

/// A captured page with its place: language, key, kind and role.
#[derive(Clone, Debug)]
pub(crate) struct Placed {
    pub key: ContentKey,
    pub kind: PageKind,
    pub role: PageRole,
    pub page: CapturedPage,
}

/// The output of phase B1.
#[derive(Debug, Default)]
pub(crate) struct Assembly {
    /// Placed pages, in capture order.
    pub pages: Vec<Placed>,
    /// Per language: key → index into `pages` (standalone pages only).
    pub trees: Vec<BTreeMap<ContentKey, usize>>,
    pub resources: Vec<CapturedResource>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Places every captured page.
pub(crate) fn place(cfg: &Config, capture: Capture) -> Result<Assembly, ModelError> {
    let Capture {
        pages,
        resources,
        diagnostics,
        ..
    } = capture;
    let mut out = Assembly {
        trees: vec![BTreeMap::new(); cfg.sites.len()],
        resources,
        diagnostics,
        ..Assembly::default()
    };
    let mut bundled = Vec::new();
    for page in pages {
        let site = &cfg.sites[page.lang];
        let info = &page.source.info;
        let key = if page.bundled {
            page.source.file_info.key.clone()
        } else {
            info.key.clone()
        };
        let kind = match page.kind {
            Some(k) => {
                if matches!(k, PageKind::Taxonomy | PageKind::Term)
                    && taxonomy_kind(site, &key).is_none()
                {
                    return Err(ModelError::NoTaxonomy {
                        path: page.source.file.abs.clone(),
                        key: key.to_path(),
                    });
                }
                k
            }
            None if page.bundled => PageKind::Page,
            None if key.is_home() => PageKind::Home,
            None if info.kind == BundleKind::Branch => {
                taxonomy_kind(site, &key).unwrap_or(PageKind::Section)
            }
            None => PageKind::Page,
        };
        if kind == PageKind::Page && site.disable_kinds.contains(PageKind::Page) {
            continue;
        }
        if kind == PageKind::Home && info.kind == BundleKind::Leaf && !page.bundled {
            out.diagnostics.push(
                Diagnostic::warning(format!(
                    "{} makes the home page a leaf bundle: it cannot have child pages or \
                     sections; rename it to _index.{}",
                    page.source.file.rel, info.ext
                ))
                .with_id("warning-home-page-is-leaf-bundle"),
            );
        }
        let idx = out.pages.len();
        if page.bundled {
            bundled.push(idx);
        } else {
            let tree = &mut out.trees[page.lang.index()];
            if let Some(&first) = tree.get(&key) {
                let first: &Placed = &out.pages[first];
                out.diagnostics.push(
                    Diagnostic::warning(format!(
                        "duplicate content path {:?}: {} is used, {} is ignored",
                        key.to_path(),
                        first.page.source.file.abs.display(),
                        page.source.file.abs.display()
                    ))
                    .with_id("duplicate-content-path"),
                );
                continue;
            }
            tree.insert(key.clone(), idx);
        }
        out.pages.push(Placed {
            key,
            kind,
            role: PageRole::Standalone,
            page,
        });
    }

    // The bundle of a content file inside a leaf bundle: the longest page key above it, in
    // any language.
    for idx in bundled {
        let key = out.pages[idx].key.clone();
        let mut k = key.parent();
        let bundle = loop {
            let Some(cur) = k else {
                break ContentKey::home();
            };
            if out.trees.iter().any(|t| t.contains_key(&cur)) {
                break cur;
            }
            k = cur.parent();
        };
        out.pages[idx].role = PageRole::Bundled { bundle };
    }
    Ok(out)
}

/// The kind of a branch bundle at `key` in a site's taxonomies: the taxonomy page at
/// `/<plural>`, a term page below it (segment-wise).
fn taxonomy_kind(site: &SiteConfig, key: &ContentKey) -> Option<PageKind> {
    site.taxonomies.iter().find_map(|t| {
        let plural = ContentKey::from_source(&t.plural);
        if plural.is_home() {
            None
        } else if *key == plural {
            Some(PageKind::Taxonomy)
        } else if key.starts_with_segments(&plural) {
            Some(PageKind::Term)
        } else {
            None
        }
    })
}
