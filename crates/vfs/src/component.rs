//! The component directories of a project.

use std::fmt;

/// A component directory: every mount targets one, and each has its own union file view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Component {
    Content,
    Layouts,
    Assets,
    Data,
    I18n,
    Static,
    Archetypes,
}

impl Component {
    /// Every component, in the order default mounts are added.
    pub const ALL: [Self; 7] = [
        Self::Content,
        Self::Data,
        Self::Layouts,
        Self::I18n,
        Self::Archetypes,
        Self::Assets,
        Self::Static,
    ];

    /// The directory name (`content`, `i18n`, …), which is also the first segment of a mount
    /// target.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Layouts => "layouts",
            Self::Assets => "assets",
            Self::Data => "data",
            Self::I18n => "i18n",
            Self::Static => "static",
            Self::Archetypes => "archetypes",
        }
    }

    /// The component named `s` (exact, lower case).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
