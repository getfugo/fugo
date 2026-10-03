//! The default order of page lists.

use std::cmp::Ordering;

use jiff::Zoned;
use ssg_base::Collate;

/// Unix seconds of Go's zero date, the date of pages without one (it sorts last).
const ZERO_DATE_SECONDS: i64 = -62_135_596_800;

/// What the default order reads from a page.
#[derive(Clone, Copy, Debug)]
pub struct SortKey<'a> {
    pub weight: i32,
    pub date: Option<&'a Zoned>,
    pub link_title: &'a str,
    /// The page's full normalised source path, with extension and language
    /// (`/posts/a.th.md`); for pages without a file, `.Path`.
    pub path: &'a str,
    /// A page of a term list: its position among the term's pages (`GetTerms` order).
    pub ordinal: Option<u32>,
    /// A page of a taxonomy list: its weight in that taxonomy (`tags_weight`).
    pub weight0: Option<i32>,
}

impl SortKey<'_> {
    fn seconds(&self) -> i64 {
        self.date
            .map_or(ZERO_DATE_SECONDS, |d| d.timestamp().as_second())
    }
}

/// Go's default page order: term ordinal, taxonomy weight, then weight ascending with 0
/// last, date descending (to the second), link title in the language's collation, and the
/// source path in byte order. Use with a stable sort.
#[must_use]
pub fn default_order(a: &SortKey<'_>, b: &SortKey<'_>, c: &dyn Collate) -> Ordering {
    if let (Some(x), Some(y)) = (a.ordinal, b.ordinal)
        && x != y
    {
        return x.cmp(&y);
    }
    if let (Some(x), Some(y)) = (a.weight0, b.weight0)
        && x != y
    {
        return x.cmp(&y);
    }
    if a.weight != b.weight {
        return match (a.weight, b.weight) {
            (_, 0) => Ordering::Less,
            (0, _) => Ordering::Greater,
            (x, y) => x.cmp(&y),
        };
    }
    b.seconds()
        .cmp(&a.seconds())
        .then_with(|| c.compare(a.link_title, b.link_title))
        .then_with(|| a.path.cmp(b.path))
}
