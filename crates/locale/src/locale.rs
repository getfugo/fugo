//! One language's locale data.

use std::cmp::Ordering;
use std::fmt;

use icu_datetime::fieldsets::enums::DateFieldSet;
use icu_datetime::preferences::CalendarAlgorithm;
use icu_datetime::{DateTimeFormatter, DateTimeFormatterPreferences};
use icu_decimal::{DecimalFormatter, DecimalFormatterPreferences};
use ssg_base::Collate;

use crate::collate::Collator;
use crate::date::{self, DateStyle, NameWidth, Names};
use crate::plural::{PluralCount, PluralForm, PluralRules};
use crate::tag;

/// The collation, plural rules, number format and date formats of one site language.
///
/// Built once per language (it loads ICU compiled data) and shared read-only; it is `Send` and
/// `Sync`, and it is the language's [`Collate`].
pub struct Locale {
    key: String,
    collator: Collator,
    plurals: PluralRules,
    decimal: DecimalFormatter,
    dates: [DateTimeFormatter<DateFieldSet>; 4],
    names: Names,
}

impl Locale {
    /// The locale of the language key `key` (`en`, `th`, `pt-br`, `zh-Hant-TW`). A language
    /// without CLDR date data (`klingon`, `x1`) formats numbers and dates as English; it
    /// collates in the root order and has the root plural rules.
    #[must_use]
    pub fn new(key: &str) -> Self {
        let icu = tag::cldr_locale(key);
        let mut date_prefs = DateTimeFormatterPreferences::from(&icu);
        // Go prints Gregorian dates in every language (Thai defaults to Buddhist in CLDR).
        date_prefs.calendar_algorithm = Some(CalendarAlgorithm::Gregory);
        let decimal = DecimalFormatter::try_new(
            DecimalFormatterPreferences::from(&icu),
            icu_decimal::options::DecimalFormatterOptions::default(),
        )
        .expect("compiled data has number symbols for every locale");
        Self {
            key: tag::normalize(key),
            collator: Collator::for_language(key),
            plurals: PluralRules::for_language(key),
            decimal,
            dates: DateStyle::ALL.map(|s| date::date_formatter(date_prefs, s)),
            names: Names::new(date_prefs),
        }
    }

    /// The normalised language key (`pt-br`).
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The collation order.
    #[must_use]
    pub fn collator(&self) -> &Collator {
        &self.collator
    }

    /// The plural form `count` selects.
    #[must_use]
    pub fn plural_form(&self, count: &PluralCount) -> PluralForm {
        self.plurals.form(count)
    }

    /// The name of month `month` (1–12).
    ///
    /// # Panics
    /// When `month` is not in 1–12.
    #[must_use]
    pub fn month_name(&self, month: i8, width: NameWidth) -> &str {
        let names = match width {
            NameWidth::Wide => &self.names.months_wide,
            NameWidth::Abbreviated => &self.names.months_abbr,
        };
        let i = usize::try_from(month - 1).expect("month is 1..=12");
        &names[i]
    }

    /// The name of a weekday.
    #[must_use]
    pub fn weekday_name(&self, day: jiff::civil::Weekday, width: NameWidth) -> &str {
        let names = match width {
            NameWidth::Wide => &self.names.weekdays_wide,
            NameWidth::Abbreviated => &self.names.weekdays_abbr,
        };
        &names[usize::from(day.to_monday_zero_offset().unsigned_abs())]
    }

    pub(crate) fn decimal(&self) -> &DecimalFormatter {
        &self.decimal
    }

    pub(crate) fn date_formatter(&self, style: DateStyle) -> &DateTimeFormatter<DateFieldSet> {
        let i = DateStyle::ALL
            .iter()
            .position(|s| *s == style)
            .expect("ALL lists every style");
        &self.dates[i]
    }
}

impl Collate for Locale {
    fn compare(&self, a: &str, b: &str) -> Ordering {
        self.collator.compare(a, b)
    }
}

impl fmt::Debug for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Locale")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}
