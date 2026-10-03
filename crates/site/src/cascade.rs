//! The cascade in force at each place of a language's tree (phase B2).
//!
//! The site's `[[cascade]]` applies at the root. A branch page (home, section, taxonomy, term)
//! with its own `cascade` hands down its entries merged over what it received (its own entries
//! win); every page receives the cascade of its nearest branch ancestor that set one. Ancestors
//! are found segment-wise.

use std::collections::BTreeMap;

use ssg_base::paths::ContentKey;
use ssg_page::Cascade;

/// The cascades handed down in one language's tree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CascadeIndex {
    /// The site's `[[cascade]]`, in force at the root.
    site: Cascade,
    /// Branch pages with their own cascade → the cascade in force below them.
    by_key: BTreeMap<ContentKey, Cascade>,
}

impl CascadeIndex {
    /// An index holding only the site's cascade.
    #[must_use]
    pub fn new(site: Cascade) -> Self {
        Self {
            site,
            by_key: BTreeMap::new(),
        }
    }

    /// Records that the branch page at `key` hands down its own entries merged over the
    /// cascade it receives. Ancestors must be added first (tree order).
    pub fn add_branch(&mut self, key: &ContentKey, own: &Cascade) {
        if own.is_empty() {
            return;
        }
        let merged = Cascade::inherit(self.received(key), own);
        self.by_key.insert(key.clone(), merged);
    }

    /// The cascade a page at `key` receives from above: that of its nearest strict ancestor
    /// with a cascade, else the site's. (The home page receives the site's.)
    #[must_use]
    pub fn received(&self, key: &ContentKey) -> &Cascade {
        let mut k = key.parent();
        while let Some(cur) = k {
            if let Some(c) = self.by_key.get(&cur) {
                return c;
            }
            k = cur.parent();
        }
        &self.site
    }

    /// The cascade in force for the page at `key` itself: its own entries merged over what it
    /// receives (only branch pages have own entries).
    #[must_use]
    pub fn in_force(&self, key: &ContentKey, own: &Cascade) -> Cascade {
        if own.is_empty() {
            self.received(key).clone()
        } else {
            Cascade::inherit(self.received(key), own)
        }
    }

    /// The branch pages that hand down a cascade of their own, with that cascade.
    pub fn branches(&self) -> impl Iterator<Item = (&ContentKey, &Cascade)> {
        self.by_key.iter()
    }
}
