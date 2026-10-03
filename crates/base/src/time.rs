//! Date strings of front matter and data (Go's accepted layouts) and the build clock.
//!
//! [`parse_date`] tries the layouts in order (written here in `strptime` notation; `ZONE` is a
//! zone abbreviation):
//!
//! | layout | example | zone |
//! |---|---|---|
//! | `%Y-%m-%d` | `2024-07-14` | `tz` |
//! | `%Y-%m-%dT%H:%M:%S` + `Z`/`±hh:mm` | `2024-07-14T17:31:59Z` | offset |
//! | `%Y-%m-%dT%H:%M:%S` | `2024-07-14T17:31:59` | `tz` |
//! | `%a, %d %b %Y %H:%M:%S ±hhmm` | `Sun, 14 Jul 2024 17:31:59 +0700` | offset |
//! | `%a, %d %b %Y %H:%M:%S ZONE` | `Sun, 14 Jul 2024 17:31:59 ICT` | `tz` |
//! | `%d %b %y %H:%M ±hhmm` | `14 Jul 24 17:31 +0700` | offset |
//! | `%d %b %y %H:%M ZONE` | `14 Jul 24 17:31 ICT` | `tz` |
//! | `%A, %d-%b-%y %H:%M:%S ZONE` | `Sunday, 14-Jul-24 17:31:59 ICT` | `tz` |
//! | `%Y-%m-%d %H:%M:%S ±hhmm ZONE` | `2024-07-14 17:31:59.5 +0700 ICT` | offset |
//! | `%Y-%m-%dT%H:%M:%S±hhmm` | `2024-07-14T17:31:59+0700` | offset |
//! | `%Y-%m-%d %H:%M:%S` + `Z`/`±hhmm` | `2024-07-14 17:31:59Z` | offset |
//! | `%Y-%m-%d %H:%M:%S` | `2024-07-14 17:31:59` | `tz` |
//! | `%a %b %e %H:%M:%S %Y` | `Sun Jul 14 17:31:59 2024` | `tz` |
//! | `%a %b %e %H:%M:%S ZONE %Y` | `Sun Jul 14 17:31:59 ICT 2024` | `tz` |
//! | `%a %b %d %H:%M:%S ±hhmm %Y` | `Sun Jul 14 17:31:59 +0700 2024` | offset |
//! | `%Y-%m-%d %H:%M:%S` + `Z`/`±hh:mm` | `2024-07-14 17:31:59+07:00` | offset |
//! | `%d %b %Y` | `14 Jul 2024` | `tz` |
//! | `%Y-%m-%d %H:%M:%S ±hh:mm` | `2024-07-14 17:31:59 +07:00` | offset |
//! | `%Y-%m-%d %H:%M:%S ±hhmm` | `2024-07-14 17:31:59 +0700` | offset |
//! | `%I:%M%p` | `5:31PM` | UTC, date 0000-01-01 |
//! | `%b %e %H:%M:%S` | `Jul 14 17:31:59` | UTC, year 0 |
//!
//! Field widths are strict: `%Y` is four digits; `%m`, `%d`, `%y`, `%M` and `%S` two; `%H` and
//! `%I` one or two; `%e` one or two after an optional space. A space in a layout matches one or
//! more spaces. Seconds may carry a fraction (`.5` or `,5`; at most nine digits are used) in
//! every layout with seconds. Month and weekday names are English and case-insensitive; the
//! weekday is not checked against the date. `%p` is `AM` or `PM`. A `ZONE` (`ICT`, `ChST`,
//! `GMT+7`, `+07`) is recognised but not interpreted: the wall-clock time is placed in `tz`, as
//! Go does. A wall-clock time in a daylight-saving gap or fold resolves to the earlier
//! instant.

use std::fmt;

use jiff::civil::DateTime;
use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, Zoned};

/// The build's notion of "now" (`--clock`), used for future/expired decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock(pub Timestamp);

impl Clock {
    /// The current system time.
    #[must_use]
    pub fn system() -> Self {
        Self(Timestamp::now())
    }

    /// The clock's instant.
    #[must_use]
    pub fn now(self) -> Timestamp {
        self.0
    }
}

/// A date string that no accepted layout matches.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub struct DateError {
    input: String,
}

impl fmt::Display for DateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} is not a date in any accepted layout", self.input)
    }
}

impl DateError {
    /// The input that could not be parsed.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }
}

/// One element of a layout.
#[derive(Clone, Copy, Debug)]
enum Field {
    /// Literal text; a space matches a run of spaces.
    Lit(&'static str),
    /// `%Y`: four digits.
    Year,
    /// `%y`: two digits, 1969–2068.
    Year2,
    /// `%m`: two digits.
    Month,
    /// `%b`: `Jan` … `Dec`.
    MonthName,
    /// `%d`: two digits.
    Day,
    /// `%e`: an optional space, then one or two digits.
    DaySpaced,
    /// `%a`: `Sun` … `Sat`, not checked.
    Weekday,
    /// `%A`: `Sunday` … `Saturday`, not checked.
    WeekdayLong,
    /// `%H`: one or two digits.
    Hour,
    /// `%I`: one or two digits, 0–12.
    Hour12,
    /// `%M`: two digits.
    Minute,
    /// `%S`: two digits, optionally followed by a fraction.
    Second,
    /// `%p`: `AM` or `PM`.
    AmPm,
    /// A zone abbreviation, recognised and ignored.
    ZoneName,
    /// A numeric UTC offset.
    Utc(OffsetForm),
}

#[derive(Clone, Copy, Debug)]
enum OffsetForm {
    /// `±hh:mm`
    Colon,
    /// `±hhmm`
    Plain,
    /// `Z` or `±hh:mm`
    ZuluOrColon,
    /// `Z` or `±hhmm`
    ZuluOrPlain,
}

/// Where the parsed wall-clock time is placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    /// In the caller's time zone.
    Local,
    /// At the parsed offset.
    Parsed,
    /// In UTC (the layouts without a year).
    Utc,
}

struct Layout {
    fields: &'static [Field],
    zone: Zone,
}

use Field::{
    AmPm, Day, DaySpaced, Hour, Hour12, Lit, Minute, Month, MonthName, Second, Utc, Weekday,
    WeekdayLong, Year, Year2, ZoneName,
};
use OffsetForm as O;

const fn layout(fields: &'static [Field], zone: Zone) -> Layout {
    Layout { fields, zone }
}

#[rustfmt::skip]
const LAYOUTS: &[Layout] = &[
    layout(&[Year, Lit("-"), Month, Lit("-"), Day], Zone::Local),
    layout(&[Year, Lit("-"), Month, Lit("-"), Day, Lit("T"), Hour, Lit(":"), Minute, Lit(":"), Second, Utc(O::ZuluOrColon)], Zone::Parsed),
    layout(&[Year, Lit("-"), Month, Lit("-"), Day, Lit("T"), Hour, Lit(":"), Minute, Lit(":"), Second], Zone::Local),
    layout(&[Weekday, Lit(", "), Day, Lit(" "), MonthName, Lit(" "), Year, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Lit(" "), Utc(O::Plain)], Zone::Parsed),
    layout(&[Weekday, Lit(", "), Day, Lit(" "), MonthName, Lit(" "), Year, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Lit(" "), ZoneName], Zone::Local),
    layout(&[Day, Lit(" "), MonthName, Lit(" "), Year2, Lit(" "), Hour, Lit(":"), Minute, Lit(" "), Utc(O::Plain)], Zone::Parsed),
    layout(&[Day, Lit(" "), MonthName, Lit(" "), Year2, Lit(" "), Hour, Lit(":"), Minute, Lit(" "), ZoneName], Zone::Local),
    layout(&[WeekdayLong, Lit(", "), Day, Lit("-"), MonthName, Lit("-"), Year2, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Lit(" "), ZoneName], Zone::Local),
    layout(&[Year, Lit("-"), Month, Lit("-"), Day, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Lit(" "), Utc(O::Plain), Lit(" "), ZoneName], Zone::Parsed),
    layout(&[Year, Lit("-"), Month, Lit("-"), Day, Lit("T"), Hour, Lit(":"), Minute, Lit(":"), Second, Utc(O::Plain)], Zone::Parsed),
    layout(&[Year, Lit("-"), Month, Lit("-"), Day, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Utc(O::ZuluOrPlain)], Zone::Parsed),
    layout(&[Year, Lit("-"), Month, Lit("-"), Day, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second], Zone::Local),
    layout(&[Weekday, Lit(" "), MonthName, Lit(" "), DaySpaced, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Lit(" "), Year], Zone::Local),
    layout(&[Weekday, Lit(" "), MonthName, Lit(" "), DaySpaced, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Lit(" "), ZoneName, Lit(" "), Year], Zone::Local),
    layout(&[Weekday, Lit(" "), MonthName, Lit(" "), Day, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Lit(" "), Utc(O::Plain), Lit(" "), Year], Zone::Parsed),
    layout(&[Year, Lit("-"), Month, Lit("-"), Day, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Utc(O::ZuluOrColon)], Zone::Parsed),
    layout(&[Day, Lit(" "), MonthName, Lit(" "), Year], Zone::Local),
    layout(&[Year, Lit("-"), Month, Lit("-"), Day, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Lit(" "), Utc(O::Colon)], Zone::Parsed),
    layout(&[Year, Lit("-"), Month, Lit("-"), Day, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second, Lit(" "), Utc(O::Plain)], Zone::Parsed),
    layout(&[Hour12, Lit(":"), Minute, AmPm], Zone::Utc),
    layout(&[MonthName, Lit(" "), DaySpaced, Lit(" "), Hour, Lit(":"), Minute, Lit(":"), Second], Zone::Utc),
];

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAYS_LONG: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// The values read by one layout.
#[derive(Default)]
struct Fields {
    year: i16,
    month: Option<i8>,
    day: Option<i8>,
    hour: i8,
    minute: i8,
    second: i8,
    nanosecond: i32,
    pm: Option<bool>,
    offset: Option<Offset>,
}

/// A cursor over the input.
struct Input<'a>(&'a [u8]);

impl Input<'_> {
    /// Exactly `n` digits.
    fn digits(&mut self, n: usize) -> Option<i32> {
        let d = self.0.get(..n)?;
        if !d.iter().all(u8::is_ascii_digit) {
            return None;
        }
        self.0 = &self.0[n..];
        Some(d.iter().fold(0, |acc, c| acc * 10 + i32::from(c - b'0')))
    }

    /// One or two digits.
    fn one_or_two(&mut self) -> Option<i32> {
        let n = self
            .0
            .iter()
            .take(2)
            .take_while(|c| c.is_ascii_digit())
            .count();
        if n == 0 { None } else { self.digits(n) }
    }

    /// A literal; a space matches a run of spaces (or the end of the input).
    fn literal(&mut self, lit: &str) -> Option<()> {
        for &c in lit.as_bytes() {
            if c == b' ' {
                if self.0.first().is_some_and(|&b| b != b' ') {
                    return None;
                }
                let n = self.0.iter().take_while(|&&b| b == b' ').count();
                self.0 = &self.0[n..];
            } else if self.0.first() == Some(&c) {
                self.0 = &self.0[1..];
            } else {
                return None;
            }
        }
        Some(())
    }

    /// The index of the first name that prefixes the input (ASCII case ignored), consumed.
    fn name(&mut self, names: &[&str]) -> Option<usize> {
        let i = names.iter().position(|n| {
            self.0.len() >= n.len() && self.0[..n.len()].eq_ignore_ascii_case(n.as_bytes())
        })?;
        self.0 = &self.0[names[i].len()..];
        Some(i)
    }

    /// An optional fraction after the seconds: `.` or `,` and digits (at most nine used).
    fn fraction(&mut self) -> i32 {
        let b = self.0;
        if b.len() < 2 || !matches!(b[0], b'.' | b',') || !b[1].is_ascii_digit() {
            return 0;
        }
        let n = b[1..].iter().take_while(|c| c.is_ascii_digit()).count();
        let used = &b[1..=n.min(9)];
        self.0 = &b[1 + n..];
        let value = used.iter().fold(0, |acc, c| acc * 10 + i32::from(c - b'0'));
        let scale = u32::try_from(9 - used.len()).expect("at most nine digits");
        value * 10_i32.pow(scale)
    }
}

fn in_range(v: i32, lo: i32, hi: i32) -> Option<i8> {
    (lo..=hi)
        .contains(&v)
        .then(|| i8::try_from(v).expect("in range"))
}

/// Parses a date string in the first accepted layout that matches all of it. Layouts without
/// an offset are placed in `tz`.
///
/// # Errors
/// When no layout matches, or the matched fields are not a valid date and time.
pub fn parse_date(s: &str, tz: &TimeZone) -> Result<Zoned, DateError> {
    LAYOUTS
        .iter()
        .find_map(|layout| parse_layout(s, layout, tz))
        .ok_or_else(|| DateError {
            input: s.to_owned(),
        })
}

fn read_field(field: Field, input: &mut Input<'_>, f: &mut Fields) -> Option<()> {
    match field {
        Lit(text) => input.literal(text)?,
        Year => f.year = i16::try_from(input.digits(4)?).ok()?,
        Year2 => {
            let yy = input.digits(2)?;
            f.year = i16::try_from(if yy >= 69 { 1900 + yy } else { 2000 + yy }).ok()?;
        }
        Month => f.month = Some(in_range(input.digits(2)?, 1, 12)?),
        MonthName => f.month = Some(i8::try_from(input.name(&MONTHS)? + 1).ok()?),
        Day => f.day = Some(in_range(input.digits(2)?, 1, 31)?),
        DaySpaced => {
            if input.0.first() == Some(&b' ') {
                input.0 = &input.0[1..];
            }
            f.day = Some(in_range(input.one_or_two()?, 1, 31)?);
        }
        Weekday => {
            input.name(&WEEKDAYS)?;
        }
        WeekdayLong => {
            input.name(&WEEKDAYS_LONG)?;
        }
        Hour => f.hour = in_range(input.one_or_two()?, 0, 23)?,
        Hour12 => f.hour = in_range(input.one_or_two()?, 0, 12)?,
        Minute => f.minute = in_range(input.digits(2)?, 0, 59)?,
        Second => {
            f.second = in_range(input.digits(2)?, 0, 59)?;
            f.nanosecond = input.fraction();
        }
        AmPm => {
            f.pm = Some(match input.0.get(..2)? {
                b"PM" => true,
                b"AM" => false,
                _ => return None,
            });
            input.0 = &input.0[2..];
        }
        ZoneName => input.0 = &input.0[zone_name_len(input.0)?..],
        Utc(form) => {
            let (offset, n) = parse_offset(input.0, form)?;
            f.offset = Some(offset);
            input.0 = &input.0[n..];
        }
    }
    Some(())
}

fn parse_layout(s: &str, layout: &Layout, tz: &TimeZone) -> Option<Zoned> {
    let mut f = Fields::default();
    let mut input = Input(s.as_bytes());
    for &field in layout.fields {
        read_field(field, &mut input, &mut f)?;
    }
    if !input.0.is_empty() {
        return None;
    }
    match f.pm {
        Some(true) if f.hour < 12 => f.hour += 12,
        Some(false) if f.hour == 12 => f.hour = 0,
        _ => {}
    }
    let dt = DateTime::new(
        f.year,
        f.month.unwrap_or(1),
        f.day.unwrap_or(1),
        f.hour,
        f.minute,
        f.second,
        f.nanosecond,
    )
    .ok()?;
    let tz = match layout.zone {
        Zone::Local => tz.clone(),
        Zone::Utc => TimeZone::UTC,
        Zone::Parsed => TimeZone::fixed(f.offset?),
    };
    tz.to_ambiguous_zoned(dt).earlier().ok()
}

/// The length of a time zone abbreviation at the start of `b`: three upper-case letters, four
/// or five ending in `T` (and `WITA`, `ChST`, `MeST`), `UTC`/`GMT` with an optional signed
/// hour offset, or a signed hour offset alone (`+07`).
fn zone_name_len(b: &[u8]) -> Option<usize> {
    if b.len() < 3 {
        return None;
    }
    if b.starts_with(b"ChST") || b.starts_with(b"MeST") {
        return Some(4);
    }
    if b.starts_with(b"UTC") {
        return Some(3);
    }
    if b.starts_with(b"GMT") {
        return Some(3 + signed_hours_len(&b[3..]).unwrap_or(0));
    }
    if matches!(b[0], b'+' | b'-') {
        return signed_hours_len(b);
    }
    let upper = b
        .iter()
        .take(6)
        .take_while(|c| c.is_ascii_uppercase())
        .count();
    match upper {
        3 => Some(3),
        4 if b[3] == b'T' || b.starts_with(b"WITA") => Some(4),
        5 if b[4] == b'T' => Some(5),
        _ => None,
    }
}

/// `±h…` with the hours at most 12: the length, `None` when absent or out of range.
fn signed_hours_len(b: &[u8]) -> Option<usize> {
    if !matches!(b.first(), Some(b'+' | b'-')) {
        return None;
    }
    let digits = b[1..].iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let hours: u64 = std::str::from_utf8(&b[1..=digits]).ok()?.parse().ok()?;
    (hours <= 12).then_some(1 + digits)
}

/// Parses a numeric offset at the start of `b`: `(offset, length)`. Hours up to 24 and
/// minutes up to 60 are accepted.
fn parse_offset(b: &[u8], form: OffsetForm) -> Option<(Offset, usize)> {
    if matches!(form, O::ZuluOrColon | O::ZuluOrPlain) && b.first() == Some(&b'Z') {
        return Some((Offset::UTC, 1));
    }
    let sign = match b.first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let mut input = Input(&b[1..]);
    let hours = input.digits(2)?;
    let (minutes, len) = match form {
        O::Colon | O::ZuluOrColon => {
            input.literal(":")?;
            (input.digits(2)?, 6)
        }
        O::Plain | O::ZuluOrPlain => (input.digits(2)?, 5),
    };
    if hours > 24 || minutes > 60 {
        return None;
    }
    Offset::from_seconds(sign * (hours * 3600 + minutes * 60))
        .ok()
        .map(|o| (o, len))
}
