//! Collation (string order for sorting) with ICU4X.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;

use icu_collator::options::CollatorOptions;
use icu_collator::{CollatorBorrowed, CollatorPreferences};
use ssg_base::Collate;

use crate::tag;

/// Languages collated with the root order instead of their CLDR tailoring.
///
/// CLDR 24 gave Thai `[reorder Thai]` and `alternate=shifted`: Thai sorts before Latin and
/// spaces and punctuation are ignored. Go (x/text, CLDR 23) sorts a Thai site's mixed lists
/// Latin first, and the root order reproduces its order for the strings of the reference
/// sites, once PAIYANNOI is placed as Go places it ([`Tailoring::PaiyannoiAsPunctuation`]).
const ROOT_ORDER: &[&str] = &["th"];

/// THAI CHARACTER PAIYANNOI (U+0E2F), the abbreviation mark (`กรุงเทพฯ`).
const PAIYANNOI: char = '\u{0E2F}';

/// The punctuation mark PAIYANNOI is collated as in Thai.
const PAIYANNOI_STAND_IN: &str = "!";

/// Changes to the ICU order that ICU4X cannot express as a runtime tailoring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tailoring {
    None,
    /// Thai: PAIYANNOI sorts as the punctuation mark `!` (before digits and letters), as in
    /// Go's Thai collation (x/text, CLDR 23); the root order has it among the Thai letters.
    /// Strings that are equal under this rule are ordered by the untailored order, so distinct
    /// strings stay distinct.
    PaiyannoiAsPunctuation,
}

/// The collation order of one language: tertiary strength, non-ignorable punctuation (the
/// CLDR defaults).
pub struct Collator {
    icu: CollatorBorrowed<'static>,
    tailoring: Tailoring,
}

impl Collator {
    /// The collator of the language `key` (root order for unknown keys and for
    /// [`ROOT_ORDER`] languages; Thai with PAIYANNOI as punctuation).
    #[must_use]
    pub fn for_language(key: &str) -> Self {
        let locale = tag::icu_locale(key);
        let language = locale.as_ref().map(|l| l.id.language.as_str());
        let tailoring = if language == Some("th") {
            Tailoring::PaiyannoiAsPunctuation
        } else {
            Tailoring::None
        };
        let locale = locale
            .filter(|l| !ROOT_ORDER.contains(&l.id.language.as_str()))
            .unwrap_or(icu_locale_core::Locale::UNKNOWN);
        let prefs = CollatorPreferences::from(&locale);
        let icu = CollatorBorrowed::try_new(prefs, CollatorOptions::default())
            .expect("compiled data has a collation for every locale (root included)");
        Self { icu, tailoring }
    }
}

/// `s` with PAIYANNOI replaced by its stand-in (borrowed when it has none).
fn paiyannoi_as_punctuation(s: &str) -> Cow<'_, str> {
    if s.contains(PAIYANNOI) {
        Cow::Owned(s.replace(PAIYANNOI, PAIYANNOI_STAND_IN))
    } else {
        Cow::Borrowed(s)
    }
}

impl Collate for Collator {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        match self.tailoring {
            Tailoring::PaiyannoiAsPunctuation if a.contains(PAIYANNOI) || b.contains(PAIYANNOI) => {
                self.icu
                    .compare(&paiyannoi_as_punctuation(a), &paiyannoi_as_punctuation(b))
                    .then_with(|| self.icu.compare(a, b))
            }
            _ => self.icu.compare(a, b),
        }
    }
}

impl fmt::Debug for Collator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Collator")
            .field("tailoring", &self.tailoring)
            .finish_non_exhaustive()
    }
}
