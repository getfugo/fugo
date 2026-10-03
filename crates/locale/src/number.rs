//! Localized number formatting (`lang.FormatNumber`).

use icu_decimal::input::Decimal;

use crate::locale::Locale;

/// `n` rounded to `precision` fraction digits (half to even on the exact binary value), with
/// the grouping and decimal separators of `locale`: `1234.5` → `1,234.50` in English with
/// precision 2. NaN and infinities print as `NaN`, `inf`, `-inf`.
#[must_use]
pub fn format_number(n: f64, precision: u8, locale: &Locale) -> String {
    // Negative zero prints as zero; a negative value that rounds to zero keeps its sign.
    let n = if n == 0.0 { 0.0 } else { n };
    let text = format!("{n:.*}", usize::from(precision));
    match Decimal::try_from_str(&text) {
        Ok(decimal) => locale.decimal().format(&decimal).to_string(),
        Err(_) => text,
    }
}
