//! Pagination arithmetic: a list of pages (or page groups) split into pagers of a fixed size,
//! the pager URLs, the `page/1/` alias, and the list `paginator()` pages by default.

use std::num::NonZeroUsize;
use std::ops::Range;
use std::sync::Arc;

use ssg_base::{OutputPath, PageId, PageKind, Value};
use ssg_config::OutputFormat;
use ssg_config::sections::PaginationConfig;
use ssg_page::{PageError, TargetPaths, UrlInputs, target_paths};

use crate::NavError;
use crate::model::NavModel;

/// A group of a grouped page list (`group_by` and friends); the key is a user value.
#[derive(Clone, Debug, PartialEq)]
pub struct PageGroup {
    pub key: Value,
    pub pages: Arc<[PageId]>,
}

/// What is paginated.
#[derive(Clone, Debug, PartialEq)]
pub enum PaginationItems {
    Pages(Arc<[PageId]>),
    Groups(Arc<[PageGroup]>),
}

/// A paginated list: the items and the pager size. Two calls paginate the same way exactly
/// when their `Pagination`s are equal.
#[derive(Clone, Debug, PartialEq)]
pub struct Pagination {
    items: PaginationItems,
    size: NonZeroUsize,
}

/// The part of a group on one pager: `range` indexes the group's pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSlice {
    pub group: usize,
    pub range: Range<usize>,
}

/// What one pager holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagerSlice {
    /// A range of the paginated pages.
    Pages(Range<usize>),
    /// Parts of consecutive groups.
    Groups(Vec<GroupSlice>),
}

/// One page of a pagination, numbered from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pager {
    pub number: usize,
    pub slice: PagerSlice,
}

impl Pager {
    /// The number of pages on this pager.
    #[must_use]
    pub fn len(&self) -> usize {
        match &self.slice {
            PagerSlice::Pages(r) => r.len(),
            PagerSlice::Groups(g) => g.iter().map(|s| s.range.len()).sum(),
        }
    }

    /// Whether the pager holds no pages (only the single pager of an empty list).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Pagination {
    /// Paginates `items` by `size` pages per pager.
    ///
    /// # Errors
    /// A pager size of 0.
    pub fn new(items: PaginationItems, size: usize) -> Result<Self, NavError> {
        let size = NonZeroUsize::new(size).ok_or(NavError::PagerSize)?;
        Ok(Self { items, size })
    }

    #[must_use]
    pub fn items(&self) -> &PaginationItems {
        &self.items
    }

    #[must_use]
    pub fn size(&self) -> usize {
        self.size.get()
    }

    /// The number of paginated pages (in all groups).
    #[must_use]
    pub fn total_items(&self) -> usize {
        match &self.items {
            PaginationItems::Pages(p) => p.len(),
            PaginationItems::Groups(g) => g.iter().map(|g| g.pages.len()).sum(),
        }
    }

    /// `.TotalPages`: the number of non-empty pagers (0 for an empty list).
    #[must_use]
    pub fn total_pages(&self) -> usize {
        self.total_items().div_ceil(self.size.get())
    }

    /// The number of pagers: [`total_pages`](Self::total_pages), but at least 1.
    #[must_use]
    pub fn pager_count(&self) -> usize {
        self.total_pages().max(1)
    }

    /// Pager `number` (1-based), if there is one.
    #[must_use]
    pub fn pager(&self, number: usize) -> Option<Pager> {
        if number == 0 || number > self.pager_count() {
            return None;
        }
        let size = self.size.get();
        let total = self.total_items();
        let lo = ((number - 1) * size).min(total);
        let hi = (lo + size).min(total);
        let slice = match &self.items {
            PaginationItems::Pages(_) => PagerSlice::Pages(lo..hi),
            PaginationItems::Groups(groups) => {
                let mut out = Vec::new();
                let mut start = 0;
                for (i, g) in groups.iter().enumerate() {
                    let end = start + g.pages.len();
                    let (a, b) = (lo.max(start), hi.min(end));
                    if a < b && g.key.is_null() {
                        // Pages without a group key are one group each (as in Go).
                        out.extend((a - start..b - start).map(|k| GroupSlice {
                            group: i,
                            range: k..k + 1,
                        }));
                    } else if a < b {
                        out.push(GroupSlice {
                            group: i,
                            range: a - start..b - start,
                        });
                    }
                    start = end;
                }
                PagerSlice::Groups(out)
            }
        };
        Some(Pager { number, slice })
    }

    /// Every pager, in order; an empty list still has one (empty) pager.
    #[must_use]
    pub fn pagers(&self) -> Vec<Pager> {
        (1..=self.pager_count())
            .filter_map(|n| self.pager(n))
            .collect()
    }

    /// The pages of a pager of a page list (empty for a grouped list).
    #[must_use]
    pub fn pages_of(&self, pager: &Pager) -> &[PageId] {
        match (&self.items, &pager.slice) {
            (PaginationItems::Pages(p), PagerSlice::Pages(r)) => &p[r.clone()],
            _ => &[],
        }
    }
}

/// The pager path element of pager `number` (`/page/2`); `None` for the first pager, which is
/// the page itself.
fn pager_element(cfg: &PaginationConfig, number: usize) -> Option<String> {
    (number > 1).then(|| format!("/{}/{number}", cfg.path))
}

/// The output file and link of pager `number` of the page whose paths `inputs` describes
/// (pager 1 is the page itself, pager N adds `/<pagination.path>/N`).
///
/// # Errors
/// As [`target_paths`].
pub fn pager_paths(
    inputs: &UrlInputs<'_>,
    cfg: &PaginationConfig,
    number: usize,
) -> Result<TargetPaths, PageError> {
    let element = pager_element(cfg, number);
    target_paths(&UrlInputs {
        pager: element.as_deref(),
        ..*inputs
    })
}

/// The redirect file written at `/<pagination.path>/1` of a paginated page in an HTML format
/// (`/posts/page/1/index.html`, `/404/page/1.html`); `None` for other formats or when
/// `pagination.disableAliases` is set.
///
/// # Errors
/// As [`target_paths`].
pub fn pager_alias(
    inputs: &UrlInputs<'_>,
    format: &OutputFormat,
    cfg: &PaginationConfig,
) -> Result<Option<OutputPath>, PageError> {
    if !format.is_html || cfg.disable_aliases {
        return Ok(None);
    }
    let element = format!("/{}/1", cfg.path);
    let t = target_paths(&UrlInputs {
        pager: Some(&element),
        ..*inputs
    })?;
    Ok(Some(t.target))
}

/// The pager size of `paginate(pages, size)`: the configured size without an argument, else
/// the argument as a positive integer (integers, floats truncated, `true` as 1, and strings in
/// Go integer syntax: `"16"`, `"0x10"`, `"8.0"`).
///
/// # Errors
/// More than one argument, or one that is not a positive integer.
pub fn resolve_pager_size(args: &[Value], configured: usize) -> Result<usize, NavError> {
    let arg = match args {
        [] => {
            return NonZeroUsize::new(configured)
                .map(NonZeroUsize::get)
                .ok_or(NavError::PagerSize);
        }
        [arg] => arg,
        _ => return Err(NavError::PagerArgs(args.len())),
    };
    let n: Option<i64> = match arg {
        Value::Int(i) => Some(*i),
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Go truncates floats to int"
        )]
        Value::Float(f) if f.is_finite() => Some(f.trunc() as i64),
        Value::Bool(b) => Some(i64::from(*b)),
        Value::String(s) => parse_go_int(s),
        _ => None,
    };
    n.filter(|n| *n > 0)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(NavError::PagerSize)
}

/// An integer in Go's base-prefixed syntax (`0x10`, `0o17`, `0b1`, `017`), with an optional
/// sign and a zero fraction (`8.0`).
fn parse_go_int(s: &str) -> Option<i64> {
    let s = match s.split_once('.') {
        Some((int, frac)) if !int.is_empty() && frac.bytes().all(|b| b == b'0') => int,
        Some(_) => return None,
        None => s,
    };
    let (neg, digits) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let lower = digits.to_ascii_lowercase();
    let (radix, body) = if let Some(b) = lower.strip_prefix("0x") {
        (16, b)
    } else if let Some(b) = lower.strip_prefix("0o") {
        (8, b)
    } else if let Some(b) = lower.strip_prefix("0b") {
        (2, b)
    } else if lower.len() > 1 && lower.starts_with('0') {
        (8, &lower[1..])
    } else {
        (10, lower.as_str())
    };
    let body = body.replace('_', "");
    if body.is_empty() || body.starts_with(['+', '-']) {
        return None;
    }
    let v = i64::from_str_radix(&body, radix).ok()?;
    Some(if neg { -v } else { v })
}

/// The list `paginator()` paginates on page `page`: the site's regular pages on the home and
/// 404 pages, `.Pages` on taxonomy and term pages, else `.RegularPages`.
#[must_use]
pub fn default_pagination_list(m: &impl NavModel, page: PageId) -> Arc<[PageId]> {
    let p = m.page(page);
    match p.kind {
        PageKind::Home | PageKind::NotFound => m.site_regular_pages(p.lang).into(),
        PageKind::Taxonomy | PageKind::Term => m.pages(page).into(),
        _ => m.regular_pages(page).into(),
    }
}
