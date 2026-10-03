//! Named mutable render state (REWRITE_PLAN.md §1.2, §2.5): the page stores and the deferred
//! registry. Owned by the render session, shared with site functions through `Arc`s.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use ssg_base::{IdVec, PageId, TxnId};

/// A buffered page-store write: page, key, value.
type Write = (PageId, String, tera::Value);

/// `.Store` of every page. Layout-phase writes (no transaction) apply directly; content-phase
/// writes are buffered per [`TxnId`] and applied by [`commit`](Self::commit) only for the
/// computation that won its memo cell ([`discard`](Self::discard) drops a loser's), so
/// duplicate work stays free of side effects.
#[derive(Debug)]
pub struct PageStores {
    stores: IdVec<PageId, RwLock<BTreeMap<String, tera::Value>>>,
    buffered: Mutex<BTreeMap<TxnId, Vec<Write>>>,
    next_txn: AtomicU64,
}

impl PageStores {
    /// Empty stores for `pages` pages.
    #[must_use]
    pub fn new(pages: usize) -> Self {
        Self {
            stores: (0..pages).map(|_| RwLock::default()).collect(),
            buffered: Mutex::default(),
            next_txn: AtomicU64::new(1),
        }
    }

    /// A new transaction for a content-phase computation.
    pub fn begin(&self) -> TxnId {
        TxnId::from_raw(self.next_txn.fetch_add(1, Ordering::Relaxed))
    }

    /// `store_set`: buffered in `txn`, else applied at once.
    pub fn set(&self, txn: Option<TxnId>, page: PageId, key: &str, value: tera::Value) {
        match txn {
            Some(t) => self
                .buffered
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(t)
                .or_default()
                .push((page, key.to_owned(), value)),
            None => {
                self.stores[page]
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(key.to_owned(), value);
            }
        }
    }

    /// `store_get`: the transaction's own latest write, else the committed value.
    #[must_use]
    pub fn get(&self, txn: Option<TxnId>, page: PageId, key: &str) -> Option<tera::Value> {
        if let Some(t) = txn {
            let b = self.buffered.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(v) = b
                .get(&t)
                .and_then(|w| w.iter().rev().find(|(p, k, _)| *p == page && k == key))
            {
                return Some(v.2.clone());
            }
        }
        self.stores[page]
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    /// Applies the writes of `txn` (the winning computation), in write order.
    pub fn commit(&self, txn: TxnId) {
        let writes = self
            .buffered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&txn);
        for (page, key, value) in writes.into_iter().flatten() {
            self.stores[page]
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(key, value);
        }
    }

    /// Drops the writes of `txn` (a computation that lost its memo cell).
    pub fn discard(&self, txn: TxnId) {
        self.buffered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&txn);
    }
}

/// A `defer` call: the template to render in phase E5 and its `data`.
#[derive(Clone, Debug, PartialEq)]
pub struct Deferred {
    /// The Tera name of the template (a `TemplateName` as a string).
    pub template: Arc<str>,
    pub data: tera::Value,
}

/// The `defer(...)` calls of wave 1, one per key (the first registration wins).
#[derive(Debug, Default)]
pub struct DeferredRegistry {
    entries: Mutex<BTreeMap<String, Deferred>>,
}

impl DeferredRegistry {
    /// Registers `d` under `key` unless the key is taken; returns the registered entry.
    pub fn register(&self, key: &str, d: Deferred) -> Deferred {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key.to_owned())
            .or_insert(d)
            .clone()
    }

    /// Every registration, by key.
    #[must_use]
    pub fn entries(&self) -> Vec<(String, Deferred)> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}
