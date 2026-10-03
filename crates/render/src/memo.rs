//! Memo cells of the content phase (REWRITE_PLAN.md §1.1, §2.6, §3.2).
//!
//! A cell never blocks: a value that is set is returned; a key already in the caller's
//! `scope.chain` is a cycle; otherwise the caller computes the value itself, with the key
//! appended to the chain and a new page-store transaction, and offers it to the cell. The
//! first offer wins and commits its transaction; a later one returns the winner's value and
//! discards its own store writes, so duplicate work stays free of side effects.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use ssg_base::{IdVec, PageId};
use ssg_markup::Fragments;
use ssg_view::{
    ContentError, ExpandedSource, HookVariant, PageStores, Phase, RenderScope, RenderedContent,
    Stage,
};

/// A memoised value: set once, never waited for.
#[derive(Debug)]
pub(crate) struct Memo<T>(OnceLock<T>);

impl<T> Default for Memo<T> {
    fn default() -> Self {
        Self(OnceLock::new())
    }
}

impl<T: Clone> Memo<T> {
    /// The value, if a computation has won the cell.
    pub(crate) fn get(&self) -> Option<T> {
        self.0.get().cloned()
    }

    /// Offers `v`; returns whether it won, and the cell's value.
    fn offer(&self, v: T) -> (bool, T) {
        match self.0.set(v) {
            Ok(()) => (true, self.0.get().cloned().expect("just set")),
            Err(_) => (false, self.0.get().cloned().expect("set by the winner")),
        }
    }
}

/// The value of `cell` for `key`, computed by `f` in a child scope of `scope` (page `key.0`
/// at `at`, a new store transaction, `key` appended to the chain) unless the cell is set.
///
/// # Errors
/// [`ContentError::Cycle`] (message from `cycle`) when `key` is in `scope.chain`; `f`'s error.
pub(crate) fn get_or_compute<T: Clone>(
    cell: &Memo<T>,
    key: (PageId, Stage),
    scope: &RenderScope,
    at: Place,
    stores: &PageStores,
    cycle: impl FnOnce(&[(PageId, Stage)]) -> String,
    f: impl FnOnce(&RenderScope) -> Result<T, ContentError>,
) -> Result<T, ContentError> {
    if let Some(v) = cell.get() {
        return Ok(v);
    }
    if scope.chain.contains(&key) {
        return Err(ContentError::Cycle(cycle(&scope.chain)));
    }
    let txn = stores.begin();
    let mut chain = scope.chain.clone();
    chain.push(key);
    let child = RenderScope {
        page: key.0,
        lang: at.lang,
        format: at.format,
        pager: None,
        phase: Phase::Content,
        variant: at.variant,
        frame: None,
        txn: Some(txn),
        depth: scope.depth,
        chain,
        adapter: scope.adapter,
    };
    match f(&child) {
        Ok(v) => {
            let (won, v) = cell.offer(v);
            if won {
                stores.commit(txn);
            } else {
                stores.discard(txn);
            }
            Ok(v)
        }
        Err(e) => {
            stores.discard(txn);
            Err(e)
        }
    }
}

/// The language, format and hook variant of a computation's scope.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Place {
    pub lang: ssg_base::LangIdx,
    pub format: ssg_base::FormatId,
    pub variant: HookVariant,
}

/// The memo cells of every page: expanded sources and rendered content per hook variant, and
/// the fragments (parse only, from the `Html` expansion).
#[derive(Debug)]
pub(crate) struct ContentStore {
    pub expanded: BTreeMap<HookVariant, IdVec<PageId, Memo<Arc<ExpandedSource>>>>,
    pub frags: IdVec<PageId, Memo<Arc<Fragments>>>,
    pub content: BTreeMap<HookVariant, IdVec<PageId, Memo<Arc<RenderedContent>>>>,
}

impl ContentStore {
    /// Empty cells for `pages` pages in `variants` (`Html` first).
    pub(crate) fn new(pages: usize, variants: &[HookVariant]) -> Self {
        fn cells<T>(pages: usize) -> IdVec<PageId, Memo<T>> {
            (0..pages).map(|_| Memo::default()).collect()
        }
        Self {
            expanded: variants.iter().map(|&v| (v, cells(pages))).collect(),
            frags: cells(pages),
            content: variants.iter().map(|&v| (v, cells(pages))).collect(),
        }
    }
}
