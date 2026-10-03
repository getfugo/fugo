//! Go's template scorer (v0.146 `templatedescriptor.go` semantics, REWRITE_PLAN.md §4.3):
//! a template's [`Desc`] is compared with the query's, giving a [`Score`]; [`Best`] keeps the
//! winner of a walk from the root key down to the query key.
//!
//! The rules, in the order they are applied:
//!
//! - **Rejections.** A plain-text template never serves an HTML query (unless the query allows
//!   plain text: `{{% %}}` shortcodes). A template's kind, language, variants and layout must
//!   match the query's when the template sets them (`all` matches any layout, and a front-matter
//!   layout matches a template layout of the same name). A template for another output format
//!   is accepted only for the same media type, and only when the kind or the layout still
//!   matches (so `page.html` serves `amp`, but `list.html` does not serve a `single` query in
//!   `amp`).
//! - **Weights** (`w1`): 1 for being compatible, kind +5, standard layout +4 (`all` +2), a
//!   front-matter layout +6, language +1 (or the default language for a template without
//!   one), output format +4, media type +1, variant1 +6, variant2 +4. `w2` is 1 when the kind or
//!   a standard layout matched and 2 for a front-matter layout; `w3` counts the matched format,
//!   media type and language.
//! - **Fallbacks.** A render hook of the right variants (but another format) and any shortcode
//!   that is compatible in plain-text terms get `w1 = 1` instead of being rejected.
//! - **Choosing** ([`Best::offer`]): a user or theme template beats an embedded one (except for
//!   render hooks); a template closer to the query path wins unless its `w2` or `w3` is
//!   lower; otherwise the higher `w1` wins, then the closer template, then the smaller path.

use std::sync::Arc;

use ssg_base::{FormatId, LangIdx, MediaTypeId, PageKind};

use crate::HookKind;

/// Standard layout names.
pub(crate) const LAYOUT_SINGLE: &str = "single";
pub(crate) const LAYOUT_LIST: &str = "list";
pub(crate) const LAYOUT_ALL: &str = "all";

/// What a template (or a query) is for, as the scorer compares it. Template descriptors are
/// derived from the file name at scan time; query descriptors from the page and format.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Desc {
    /// One of the five main kinds (home, section, page, taxonomy, term).
    pub kind: Option<PageKind>,
    /// The template's layout identifier (`single`, `list`, `all`, or a custom name); for a
    /// query, `single` (pages) or `list` (other kinds).
    pub layout: Option<Arc<str>>,
    /// Query only: the layout set in front matter.
    pub user_layout: Option<Arc<str>>,
    pub lang: Option<LangIdx>,
    pub format: Option<FormatId>,
    pub media: Option<MediaTypeId>,
    /// Render hooks: the hook kind and its variant (`codeblock` + `go`).
    pub variant1: Option<HookKind>,
    pub variant2: Option<Arc<str>>,
    pub flags: Flags,
}

/// The boolean parts of a descriptor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Flags {
    /// The template (or the query's output format) is plain text.
    pub plain: bool,
    /// Query only: only a template whose layout is exactly [`Desc::user_layout`] matches.
    pub exact_layout: bool,
    /// Query only: plain-text templates may serve an HTML query (`{{% %}}` shortcodes).
    pub allow_plain: bool,
}

/// Which lookup a comparison is made for; the fallbacks and the format rule differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    Layout,
    Base,
    Hook,
    Shortcode,
    Partial,
}

/// How well a template matches a query (Go's weights). Compared with [`Best::offer`], not by
/// a total order: a template closer to the query path can win with a lower `w1`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Score {
    /// The main weight; `<= 0` is no match.
    pub(crate) w1: i32,
    /// 1: the kind or a standard layout matched; 2: a front-matter layout matched.
    pub(crate) w2: i32,
    /// The number of matched format, media type and language.
    pub(crate) w3: i32,
    /// The number of path segments between the template's key and the query's.
    pub(crate) distance: usize,
}

impl Score {
    const NO_MATCH: Self = Self {
        w1: -1,
        w2: 0,
        w3: 0,
        distance: 0,
    };

    /// Whether the template matches at all.
    #[must_use]
    pub fn matches(self) -> bool {
        self.w1 > 0
    }

    fn same_weights(self, other: Self) -> bool {
        self.w1 == other.w1 && self.w2 == other.w2 && self.w3 == other.w3
    }
}

/// The site-wide values the comparison needs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Defaults {
    pub lang: LangIdx,
    pub format: Option<FormatId>,
}

/// Whether a base template of kind `base_kind` may wrap a layout with layout identifier
/// `layout` (a kind-specific base never wraps the other standard layout).
pub(crate) fn kind_fits_layout(base_kind: Option<PageKind>, layout: Option<&str>) -> bool {
    let layout = layout.unwrap_or_default();
    match base_kind {
        None => true,
        Some(PageKind::Page) => layout != LAYOUT_LIST,
        Some(_) => layout != LAYOUT_SINGLE,
    }
}

/// Compares the template descriptor `t` with the query descriptor `q`.
pub(crate) fn compare(defaults: Defaults, cat: Category, q: &Desc, t: &Desc) -> Score {
    if q.flags.exact_layout && q.user_layout != t.layout {
        return Score::NO_MATCH;
    }
    let mut w = weigh(defaults, cat, q, t);
    if w.w1 <= 0 {
        match cat {
            Category::Hook
                if q.variant1 == t.variant1
                    && (q.variant2 == t.variant2
                        || (q.variant2.is_some() && t.variant2.is_none())) =>
            {
                // A hook for another format is a fallback, except when the query is for the
                // default format (the other format's hook would change the main output).
                if q.format != t.format && q.format == defaults.format {
                    return w;
                }
                w.w1 = 1;
            }
            Category::Shortcode
                if q.flags.plain == t.flags.plain || !t.flags.plain || q.flags.allow_plain =>
            {
                w.w1 = 1;
            }
            _ => {}
        }
    }
    w
}

fn weigh(defaults: Defaults, cat: Category, q: &Desc, t: &Desc) -> Score {
    const KIND: i32 = 5;
    const CUSTOM_LAYOUT: i32 = 6;
    const STANDARD_LAYOUT: i32 = 4;
    const ALL_LAYOUT: i32 = 2;
    const FORMAT: i32 = 4;
    const MEDIA: i32 = 1;
    const LANG: i32 = 1;
    const VARIANT1: i32 = 6;
    const VARIANT2: i32 = 4;

    let no = Score::NO_MATCH;
    // HTML in plain text is fine, not the other way around.
    if !q.flags.allow_plain && t.flags.plain && !q.flags.plain {
        return no;
    }
    if t.kind.is_some() && t.kind != q.kind {
        return no;
    }
    let t_layout = t.layout.as_deref();
    if let Some(tl) = t_layout
        && tl != LAYOUT_ALL
        && q.user_layout.as_deref() != Some(tl)
        && q.layout.as_deref() != Some(tl)
    {
        return no;
    }
    if t.lang.is_some() && t.lang != q.lang {
        return no;
    }
    if t.format.is_some() && t.format != q.format {
        if q.media != t.media {
            return no;
        }
        // Another format of the same media type (amp for html) serves the query when the kind
        // or the layout still matches.
        let mut skip = cat != Category::Base
            && (q.kind.is_none()
                || (q.kind != t.kind
                    && q.layout.as_deref() != t_layout
                    && t_layout != Some(LAYOUT_ALL)));
        if q.user_layout.is_some() {
            skip = skip && q.user_layout.as_deref() != t_layout;
        }
        if skip {
            return no;
        }
    }
    if t.media != q.media {
        return no;
    }
    if t.variant1.is_some() {
        if t.variant1 != q.variant1 {
            return no;
        }
        if t.variant2.is_some() && t.variant2 != q.variant2 {
            return no;
        }
    }

    let mut w = Score {
        w1: 1,
        ..Score::default()
    };
    if t.kind.is_some() && t.kind == q.kind {
        w.w1 += KIND;
        w.w2 = 1;
    }
    if t_layout.is_some() && t_layout == q.layout.as_deref() {
        w.w1 += STANDARD_LAYOUT;
        w.w2 = 1;
    } else if t_layout == Some(LAYOUT_ALL) {
        w.w1 += ALL_LAYOUT;
        w.w2 = 1;
    }
    if q.user_layout.is_some() && q.user_layout.as_deref() == t_layout {
        w.w1 += CUSTOM_LAYOUT;
        w.w2 = 2;
    }
    if (t.lang.is_some() && t.lang == q.lang) || (t.lang.is_none() && q.lang == Some(defaults.lang))
    {
        w.w1 += LANG;
        w.w3 += 1;
    }
    if t.format.is_some() && t.format == q.format {
        w.w1 += FORMAT;
        w.w3 += 1;
    }
    if t.media.is_some() && t.media == q.media {
        w.w1 += MEDIA;
        w.w3 += 1;
    }
    if t.variant1.is_some() && t.variant1 == q.variant1 {
        w.w1 += VARIANT1;
    }
    if t.variant1.is_some() && t.variant2 == q.variant2 {
        w.w1 += VARIANT2;
    }
    w
}

/// A candidate as the chooser sees it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Candidate<'a, T> {
    pub item: T,
    /// The path the tie-break compares (the template's file path).
    pub path: &'a str,
    pub embedded: bool,
    /// Render hooks do not prefer user templates over embedded ones.
    pub hook: bool,
}

/// The best candidate so far.
#[derive(Debug)]
pub(crate) struct Best<'a, T> {
    best: Option<(Candidate<'a, T>, Score)>,
}

impl<'a, T: Copy> Best<'a, T> {
    pub fn new() -> Self {
        Self { best: None }
    }

    /// Offers a candidate; it replaces the current best if it is better.
    pub fn offer(&mut self, c: Candidate<'a, T>, w: Score) {
        if self.is_better(&c, w) {
            self.best = Some((c, w));
        }
    }

    fn is_better(&self, c: &Candidate<'a, T>, w: Score) -> bool {
        let Some((best, bw)) = &self.best else {
            return true;
        };
        if w.w1 <= 0 {
            return bw.w1 <= 0 && c.path < best.path;
        }
        if !best.hook && bw.w1 > 0 && best.embedded != c.embedded {
            // A user or theme template beats an embedded one.
            return best.embedded;
        }
        if w.distance < bw.distance {
            if w.w2 < bw.w2 || w.w3 < bw.w3 {
                return false;
            }
        } else if w.w1 < bw.w1 {
            return false;
        }
        if w.same_weights(*bw) {
            if w.distance < bw.distance {
                return true;
            }
            return c.path < best.path;
        }
        true
    }

    /// The best candidate, matching or not.
    pub fn any(&self) -> Option<T> {
        self.best.as_ref().map(|(c, _)| c.item)
    }

    /// The winner, if it matches.
    pub fn winner(&self) -> Option<(T, Score)> {
        self.best
            .as_ref()
            .filter(|(_, w)| w.matches())
            .map(|(c, w)| (c.item, *w))
    }
}
