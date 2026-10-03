//! Page kinds and sets of them.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The kind of a page. The first five are the content kinds of `.Site.Pages`; the others are
/// rendered in isolation (sitemap, robots.txt, 404).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    Home,
    Section,
    Page,
    Taxonomy,
    Term,
    #[serde(rename = "404")]
    NotFound,
    Sitemap,
    SitemapIndex,
    #[serde(rename = "robotstxt")]
    RobotsTxt,
}

impl PageKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 9] = [
        Self::Home,
        Self::Section,
        Self::Page,
        Self::Taxonomy,
        Self::Term,
        Self::NotFound,
        Self::Sitemap,
        Self::SitemapIndex,
        Self::RobotsTxt,
    ];

    /// The name templates and configuration use (`"home"`, `"404"`, `"robotstxt"`, …).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Section => "section",
            Self::Page => "page",
            Self::Taxonomy => "taxonomy",
            Self::Term => "term",
            Self::NotFound => "404",
            Self::Sitemap => "sitemap",
            Self::SitemapIndex => "sitemapindex",
            Self::RobotsTxt => "robotstxt",
        }
    }

    /// Parses a kind name, ignoring ASCII case. The legacy name `taxonomyTerm` (before Go
    /// version 0.73) means [`PageKind::Taxonomy`].
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.to_ascii_lowercase();
        if s == "taxonomyterm" {
            return Some(Self::Taxonomy);
        }
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// Home, section, taxonomy and term pages: the kinds that have children.
    #[must_use]
    pub const fn is_branch(self) -> bool {
        matches!(
            self,
            Self::Home | Self::Section | Self::Taxonomy | Self::Term
        )
    }

    /// The kinds that live in the content trees and appear in `.Site.Pages`.
    #[must_use]
    pub const fn is_content(self) -> bool {
        matches!(
            self,
            Self::Home | Self::Section | Self::Page | Self::Taxonomy | Self::Term
        )
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

impl fmt::Display for PageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A set of [`PageKind`]s (for example `disableKinds`).
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct KindSet(u16);

impl KindSet {
    /// The empty set.
    pub const EMPTY: Self = Self(0);
    /// Every kind.
    pub const ALL: Self = Self((1 << PageKind::ALL.len()) - 1);

    /// The set of the given kinds.
    #[must_use]
    pub const fn of(kinds: &[PageKind]) -> Self {
        let mut bits = 0;
        let mut i = 0;
        while i < kinds.len() {
            bits |= kinds[i].bit();
            i += 1;
        }
        Self(bits)
    }

    /// Whether `kind` is in the set.
    #[must_use]
    pub const fn contains(self, kind: PageKind) -> bool {
        self.0 & kind.bit() != 0
    }

    /// Adds `kind`.
    pub fn insert(&mut self, kind: PageKind) {
        self.0 |= kind.bit();
    }

    /// Removes `kind`.
    pub fn remove(&mut self, kind: PageKind) {
        self.0 &= !kind.bit();
    }

    /// The set with `kind` added.
    #[must_use]
    pub const fn with(self, kind: PageKind) -> Self {
        Self(self.0 | kind.bit())
    }

    /// The union of two sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether the set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The number of kinds in the set.
    #[must_use]
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// The kinds of the set, in declaration order.
    pub fn iter(self) -> impl Iterator<Item = PageKind> {
        PageKind::ALL.into_iter().filter(move |k| self.contains(*k))
    }
}

impl FromIterator<PageKind> for KindSet {
    fn from_iter<I: IntoIterator<Item = PageKind>>(iter: I) -> Self {
        iter.into_iter().fold(Self::EMPTY, Self::with)
    }
}

impl fmt::Debug for KindSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}
