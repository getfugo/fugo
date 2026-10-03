//! Localized dates, always in the Gregorian calendar (`th` is `th-u-ca-gregory`: Thai sites
//! built by the Go implementation print Gregorian years with Thai month names).

use icu_datetime::fieldsets::{self, enums::DateFieldSet};
use icu_datetime::input::Date;
use icu_datetime::options::YearStyle;
use icu_datetime::pattern::{DateTimePattern, FixedCalendarDateTimeNames};
use icu_datetime::{DateTimeFormatter, DateTimeFormatterPreferences};
use writeable::TryWriteable as _;

use crate::locale::Locale;

/// The CLDR date styles (Go's `:date_short` … `:date_full`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DateStyle {
    Short,
    Medium,
    Long,
    Full,
}

impl DateStyle {
    /// All styles, shortest first.
    pub const ALL: [Self; 4] = [Self::Short, Self::Medium, Self::Long, Self::Full];
}

/// How a date is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatePattern<'a> {
    /// A strftime pattern (jiff's conversions); `%B`, `%b`/`%h`, `%A` and `%a` are the
    /// language's month and weekday names.
    Strftime(&'a str),
    /// A CLDR date style.
    Style(DateStyle),
}

/// The width of a month or weekday name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameWidth {
    /// `January`, `Monday`.
    Wide,
    /// `Jan`, `Mon`.
    Abbreviated,
}

/// A date that cannot be formatted.
#[derive(Debug, thiserror::Error)]
pub enum DateFormatError {
    /// The strftime pattern is invalid.
    #[error("date format `{format}`: {source}")]
    Strftime {
        format: String,
        #[source]
        source: jiff::Error,
    },
    /// The year is outside the range ICU formats.
    #[error("year {year} is out of range")]
    Range { year: i16 },
}

/// Formats `d` (in its own time zone) with `pattern` in `locale`'s language.
///
/// # Errors
/// An invalid strftime pattern, or a year ICU cannot format.
pub fn format_date(
    d: &jiff::Zoned,
    pattern: DatePattern<'_>,
    locale: &Locale,
) -> Result<String, DateFormatError> {
    match pattern {
        DatePattern::Strftime(format) => {
            let localized = localize_strftime(format, d.date(), locale);
            jiff::fmt::strtime::format(localized.as_bytes(), d).map_err(|source| {
                DateFormatError::Strftime {
                    format: format.to_owned(),
                    source,
                }
            })
        }
        DatePattern::Style(style) => {
            let year = d.year();
            let date = Date::try_new_iso(
                i32::from(year),
                u8::try_from(d.month()).expect("month is 1..=12"),
                u8::try_from(d.day()).expect("day is 1..=31"),
            )
            .map_err(|_| DateFormatError::Range { year })?;
            Ok(locale.date_formatter(style).format(&date).to_string())
        }
    }
}

/// Replaces the name conversions of a strftime pattern by the localized names (escaped for
/// strftime), leaving every other conversion to jiff.
fn localize_strftime(format: &str, date: jiff::civil::Date, locale: &Locale) -> String {
    let mut out = String::with_capacity(format.len());
    let mut chars = format.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        // flags and width, then the conversion letter
        let mut end = start + 1;
        let mut upper = false;
        let mut conversion = None;
        for (i, n) in chars.by_ref() {
            end = i + n.len_utf8();
            match n {
                '^' => upper = true,
                '-' | '_' | '0' | '#' => {}
                d if d.is_ascii_digit() => {}
                other => {
                    conversion = Some(other);
                    break;
                }
            }
        }
        let name = match conversion {
            Some('B') => Some(locale.month_name(date.month(), NameWidth::Wide)),
            Some('b' | 'h') => Some(locale.month_name(date.month(), NameWidth::Abbreviated)),
            Some('A') => Some(locale.weekday_name(date.weekday(), NameWidth::Wide)),
            Some('a') => Some(locale.weekday_name(date.weekday(), NameWidth::Abbreviated)),
            _ => None,
        };
        match name {
            Some(name) if upper => out.push_str(&name.to_uppercase().replace('%', "%%")),
            Some(name) => out.push_str(&name.replace('%', "%%")),
            None => out.push_str(&format[start..end]),
        }
    }
    out
}

/// The date formatter of a style.
///
/// The medium, long and full styles print the year in full and add no era of their own to
/// early years (`Jan 1, 1`, Go's zero time, not ICU's `Jan 1, 1 AD`), as Go's CLDR patterns
/// do; an era the language's pattern has (Thai `ค.ศ.`) stays.
pub(crate) fn date_formatter(
    prefs: DateTimeFormatterPreferences,
    style: DateStyle,
) -> DateTimeFormatter<DateFieldSet> {
    let fields = match style {
        DateStyle::Short => DateFieldSet::YMD(fieldsets::YMD::short()),
        DateStyle::Medium => {
            DateFieldSet::YMD(fieldsets::YMD::medium().with_year_style(YearStyle::NoEra))
        }
        DateStyle::Long => {
            DateFieldSet::YMD(fieldsets::YMD::long().with_year_style(YearStyle::NoEra))
        }
        DateStyle::Full => {
            DateFieldSet::YMDE(fieldsets::YMDE::long().with_year_style(YearStyle::NoEra))
        }
    };
    DateTimeFormatter::try_new(prefs, fields).expect("compiled data has every date style")
}

/// Month and weekday names of a language (format context).
#[derive(Debug)]
pub(crate) struct Names {
    /// January … December.
    pub months_wide: Vec<String>,
    pub months_abbr: Vec<String>,
    /// Monday … Sunday.
    pub weekdays_wide: Vec<String>,
    pub weekdays_abbr: Vec<String>,
}

impl Names {
    /// The Gregorian names of `prefs`' language in the format context (`MMMM`, `MMM`, `EEEE`,
    /// `EEE`: the forms used inside a date).
    pub(crate) fn new(prefs: DateTimeFormatterPreferences) -> Self {
        let format = |pattern: &str, dates: &mut dyn Iterator<Item = (u8, u8)>| {
            let pattern = DateTimePattern::try_from_pattern_str(pattern).expect("valid pattern");
            let mut names = FixedCalendarDateTimeNames::<_, DateFieldSet>::try_new(prefs)
                .expect("compiled data has date names");
            let formatter = names
                .include_for_pattern(&pattern)
                .expect("compiled data has month and weekday names");
            dates
                .map(|(month, day)| {
                    let d = Date::try_new_gregorian(2021, month, day).expect("valid date");
                    formatter
                        .format(&d)
                        .try_write_to_string()
                        .unwrap_or_else(|(_, partial)| partial)
                        .into_owned()
                })
                .collect::<Vec<_>>()
        };
        // 2021-02-01 is a Monday.
        Self {
            months_wide: format("MMMM", &mut (1..=12).map(|m| (m, 1))),
            months_abbr: format("MMM", &mut (1..=12).map(|m| (m, 1))),
            weekdays_wide: format("EEEE", &mut (1..=7).map(|d| (2, d))),
            weekdays_abbr: format("EEE", &mut (1..=7).map(|d| (2, d))),
        }
    }
}
