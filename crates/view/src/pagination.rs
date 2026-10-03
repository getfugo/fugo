//! The pagination recorder (REWRITE_PLAN.md §3.3): the first `paginator()` / `paginate()` call
//! per (page, format) records its [`Pagination`] and template position; wave 2 renders pagers
//! 2..N from the records.
//!
//! | Later call | Result |
//! |---|---|
//! | `paginator()` ([`PaginationRecorder::paginator`]) | the stored pagination |
//! | `paginate` with an equal pagination ([`PaginationRecorder::paginate`]) | the stored pagination |
//! | `paginate` with another list or size | [`PaginationConflict`] naming both positions |

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use ssg_base::diag::Position;
use ssg_base::{FormatId, PageId};
use ssg_nav::{Pagination, PaginationItems};

/// A recorded pagination and where it was first asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Recorded {
    pub pagination: Arc<Pagination>,
    /// The template position of the first call (`None`: unknown).
    pub first_call: Option<Position>,
}

impl Recorded {
    /// The number of pagers (at least 1, also for an empty list).
    #[must_use]
    pub fn total_pages(&self) -> u32 {
        u32::try_from(self.pagination.pager_count()).unwrap_or(u32::MAX)
    }

    /// The pages of pager `number` (1-based; empty past the last pager). Grouped paginations
    /// give the pages of the groups' slices in order.
    #[must_use]
    pub fn page(&self, number: u32) -> Vec<PageId> {
        let n = usize::try_from(number).unwrap_or(usize::MAX);
        let Some(pager) = self.pagination.pager(n) else {
            return Vec::new();
        };
        match self.pagination.items() {
            PaginationItems::Pages(_) => self.pagination.pages_of(&pager).to_vec(),
            PaginationItems::Groups(groups) => match &pager.slice {
                ssg_nav::PagerSlice::Groups(slices) => slices
                    .iter()
                    .flat_map(|s| groups[s.group].pages[s.range.clone()].iter().copied())
                    .collect(),
                ssg_nav::PagerSlice::Pages(r) => groups
                    .iter()
                    .flat_map(|g| g.pages.iter().copied())
                    .skip(r.start)
                    .take(r.len())
                    .collect(),
            },
        }
    }
}

/// A `paginate` call whose list or size differs from the recorded one.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub struct PaginationConflict {
    pub first: Option<Position>,
    pub second: Option<Position>,
}

impl fmt::Display for PaginationConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = |p: &Option<Position>| {
            p.as_ref()
                .map_or_else(|| "?".to_owned(), ToString::to_string)
        };
        write!(
            f,
            "this page was already paginated with another list or size at {}; paginate at {} must \
             use the same list and size (or call paginator())",
            at(&self.first),
            at(&self.second)
        )
    }
}

/// The paginations recorded during wave 1, per (page, format).
#[derive(Debug, Default)]
pub struct PaginationRecorder {
    recorded: Mutex<BTreeMap<(PageId, FormatId), Arc<Recorded>>>,
}

impl PaginationRecorder {
    /// `paginator()`: the recorded pagination of (`page`, `format`), recording `make()` (the
    /// default list) on the first call.
    pub fn paginator(
        &self,
        page: PageId,
        format: FormatId,
        at: Option<Position>,
        make: impl FnOnce() -> Pagination,
    ) -> Arc<Recorded> {
        let mut m = self.recorded.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(m.entry((page, format)).or_insert_with(|| {
            Arc::new(Recorded {
                pagination: Arc::new(make()),
                first_call: at,
            })
        }))
    }

    /// `paginate(pages=, size=)`: records `p` on the first call; a later call must paginate
    /// the same way.
    ///
    /// # Errors
    /// A recorded pagination with another list or size.
    pub fn paginate(
        &self,
        page: PageId,
        format: FormatId,
        at: Option<Position>,
        p: Pagination,
    ) -> Result<Arc<Recorded>, PaginationConflict> {
        let mut m = self.recorded.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(r) = m.get(&(page, format)) {
            if *r.pagination == p {
                return Ok(Arc::clone(r));
            }
            return Err(PaginationConflict {
                first: r.first_call.clone(),
                second: at,
            });
        }
        let r = Arc::new(Recorded {
            pagination: Arc::new(p),
            first_call: at,
        });
        m.insert((page, format), Arc::clone(&r));
        Ok(r)
    }

    /// The recorded pagination of (`page`, `format`), if any.
    #[must_use]
    pub fn get(&self, page: PageId, format: FormatId) -> Option<Arc<Recorded>> {
        let m = self.recorded.lock().unwrap_or_else(PoisonError::into_inner);
        m.get(&(page, format)).cloned()
    }

    /// Every recorded pagination, sorted by (page, format).
    #[must_use]
    pub fn recorded(&self) -> Vec<((PageId, FormatId), Arc<Recorded>)> {
        let m = self.recorded.lock().unwrap_or_else(PoisonError::into_inner);
        m.iter().map(|(k, v)| (*k, Arc::clone(v))).collect()
    }
}
