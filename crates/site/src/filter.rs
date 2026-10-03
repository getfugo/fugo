//! Phase B2, last step: drafts, future and expired content, and disabled kinds.
//!
//! A page that should not be built is removed, with the bundle resources below it in its
//! language, except home, section and taxonomy pages: they keep the tree's structure and are
//! only switched off (neither listed nor rendered, resources not published).

use ssg_base::PageKind;
use ssg_page::{BuildPolicy, ListMode, PageMeta, RenderMode};

use crate::LoadModelOptions;

/// What happens to a page that fails the filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Build,
    /// Kept for the structure, not listed or rendered.
    Disable,
    /// Removed with its bundle resources.
    Remove,
}

impl LoadModelOptions {
    /// Whether a page with these meta passes: not a draft, not scheduled after the clock, not
    /// expired before it (each unless the build includes such content).
    #[must_use]
    pub fn admits(&self, meta: &PageMeta) -> bool {
        let now = self.clock.0;
        if meta.draft && !self.content.drafts {
            return false;
        }
        if !self.content.future
            && meta
                .dates
                .publish_date
                .as_ref()
                .is_some_and(|d| d.timestamp() > now)
        {
            return false;
        }
        if !self.content.expired
            && meta
                .dates
                .expiry_date
                .as_ref()
                .is_some_and(|d| d.timestamp() < now)
        {
            return false;
        }
        true
    }

    /// The verdict for a page of `kind` whose kind is enabled or not.
    pub(crate) fn verdict(&self, kind: PageKind, kind_enabled: bool, meta: &PageMeta) -> Verdict {
        if kind_enabled && self.admits(meta) {
            return Verdict::Build;
        }
        match kind {
            PageKind::Home | PageKind::Section | PageKind::Taxonomy => Verdict::Disable,
            _ => Verdict::Remove,
        }
    }
}

/// The build policy of a switched-off node.
pub(crate) const DISABLED: BuildPolicy = BuildPolicy {
    list: ListMode::Never,
    render: RenderMode::Never,
    publish_resources: false,
};
