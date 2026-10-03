//! CLDR cardinal plural rules and plural counts.

use std::fmt;

use icu_decimal::input::Decimal;
use icu_plurals::{PluralCategory, PluralOperands, PluralRulesPreferences};
use ssg_base::Value;

use crate::tag;

/// A CLDR plural category: which variant of a message is used for a count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PluralForm {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralForm {
    /// All forms, in CLDR order.
    pub const ALL: [Self; 6] = [
        Self::Zero,
        Self::One,
        Self::Two,
        Self::Few,
        Self::Many,
        Self::Other,
    ];

    /// The CLDR keyword (`one`, `other`, …), which is also the key in message files.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::Two => "two",
            Self::Few => "few",
            Self::Many => "many",
            Self::Other => "other",
        }
    }

    /// The form named by a CLDR keyword (lower case).
    #[must_use]
    pub fn from_keyword(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == s)
    }

    const fn from_category(c: PluralCategory) -> Self {
        match c {
            PluralCategory::Zero => Self::Zero,
            PluralCategory::One => Self::One,
            PluralCategory::Two => Self::Two,
            PluralCategory::Few => Self::Few,
            PluralCategory::Many => Self::Many,
            PluralCategory::Other => Self::Other,
        }
    }
}

impl fmt::Display for PluralForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A number that selects a plural form, kept as its decimal text so that visible fraction
/// digits count: `1` is `one` in English, `1.0` is `other`.
#[derive(Clone, Debug)]
pub struct PluralCount {
    text: String,
    decimal: Decimal,
}

impl PluralCount {
    /// An integer count.
    #[must_use]
    pub fn from_int(n: i64) -> Self {
        let text = n.to_string();
        let decimal = Decimal::from(n);
        Self { text, decimal }
    }

    /// A float count, written with at least one fraction digit (`1.0`, `2.5`), the way Go
    /// turns template floats into counts. `None` for NaN and infinities.
    #[must_use]
    pub fn from_f64(f: f64) -> Option<Self> {
        if !f.is_finite() {
            return None;
        }
        // `Display` of f64 is the shortest round-trip decimal without an exponent.
        let mut text = f.to_string();
        if !text.contains('.') {
            text.push_str(".0");
        }
        Self::parse(&text)
    }

    /// A decimal string count: an optional `-`, digits, and an optional fraction (`1`, `-2`,
    /// `1.50`, `1.`). Anything else (`1e3`, ` 1`, `abc`) is not a count.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let digits = s.strip_prefix('-').unwrap_or(s);
        let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
        let all_digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
        if int.is_empty() || !all_digits(int) || !all_digits(frac) {
            return None;
        }
        let text = s.strip_suffix('.').unwrap_or(s).to_owned();
        let decimal = Decimal::try_from_str(&text).ok()?;
        Some(Self { text, decimal })
    }

    /// The count carried by a template argument, following Go: an integer; a float; a
    /// numeric string; or, in a map, the value of its `Count` key (matched ignoring case).
    /// Anything else carries no count (and selects the `other` form).
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Int(i) => Some(Self::from_int(*i)),
            Value::Float(f) => Self::from_f64(*f),
            Value::String(s) => Self::parse(s),
            Value::Map(m) => m
                .get("Count")
                .or_else(|| {
                    m.iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("count"))
                        .map(|(_, v)| v)
                })
                .and_then(|c| match c {
                    Value::Map(_) => None,
                    c => Self::from_value(c),
                }),
            _ => None,
        }
    }

    /// The count as written (`2`, `1.0`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    fn operands(&self) -> PluralOperands {
        PluralOperands::from(&self.decimal)
    }
}

impl PartialEq for PluralCount {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for PluralCount {}

impl fmt::Display for PluralCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// The cardinal plural rules of one language.
#[derive(Debug)]
pub struct PluralRules(icu_plurals::PluralRules);

impl PluralRules {
    /// The CLDR rules of the language `key`. A language CLDR does not know (`klingon`, `x1`)
    /// gets the root rules: every count is `other` (Go used the English rules).
    #[must_use]
    pub fn for_language(key: &str) -> Self {
        let locale = tag::icu_locale(key).unwrap_or(icu_locale_core::Locale::UNKNOWN);
        let prefs = PluralRulesPreferences::from(&locale);
        let rules = icu_plurals::PluralRules::try_new_cardinal(prefs)
            .expect("compiled data has plural rules for every locale (root included)");
        Self(rules)
    }

    /// The form `count` selects.
    #[must_use]
    pub fn form(&self, count: &PluralCount) -> PluralForm {
        PluralForm::from_category(self.0.category_for(count.operands()))
    }
}
