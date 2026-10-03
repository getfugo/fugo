//! Unicode helpers shared by the path, anchor, inflection and title functions.
//!
//! Character classes are Unicode general categories (via `unicode-properties`); case mappings
//! are the *simple* (one character to one character) mappings, which is what Go's rules are
//! defined over.

use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;
use unicode_properties::{GeneralCategory, GeneralCategoryGroup, UnicodeGeneralCategory};

/// General category `L*`.
#[must_use]
pub fn is_letter(c: char) -> bool {
    c.general_category_group() == GeneralCategoryGroup::Letter
}

/// General category `Lu`.
#[must_use]
pub fn is_upper(c: char) -> bool {
    c.general_category() == GeneralCategory::UppercaseLetter
}

/// General category `Nd`.
#[must_use]
pub fn is_digit(c: char) -> bool {
    c.general_category() == GeneralCategory::DecimalNumber
}

/// General category `N*`.
#[must_use]
pub fn is_number(c: char) -> bool {
    c.general_category_group() == GeneralCategoryGroup::Number
}

/// General category `M*`.
#[must_use]
pub fn is_mark(c: char) -> bool {
    c.general_category_group() == GeneralCategoryGroup::Mark
}

/// General category `P*`.
#[must_use]
pub fn is_punct(c: char) -> bool {
    c.general_category_group() == GeneralCategoryGroup::Punctuation
}

/// The simple lower-case mapping of `c` (no context, one character).
#[must_use]
pub fn lower_char(c: char) -> char {
    // `char::to_lowercase` only yields several characters for U+0130, whose simple mapping is
    // its first character ('i').
    c.to_lowercase().next().unwrap_or(c)
}

/// The simple upper-case mapping of `c`: the character itself when the full mapping expands
/// (`ß` → `SS`), since there is no single-character upper case then.
#[must_use]
pub fn upper_char(c: char) -> char {
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// The simple title-case mapping of `c`: the upper case, except for the four Latin digraphs
/// that have a distinct title-case form (`ǆ` → `ǅ`).
#[must_use]
pub fn title_char(c: char) -> char {
    match c {
        '\u{01C4}'..='\u{01C6}' => '\u{01C5}',
        '\u{01C7}'..='\u{01C9}' => '\u{01C8}',
        '\u{01CA}'..='\u{01CC}' => '\u{01CB}',
        '\u{01F1}'..='\u{01F3}' => '\u{01F2}',
        _ => upper_char(c),
    }
}

/// `s` with every character mapped by [`lower_char`] (no final-sigma rule).
#[must_use]
pub fn to_lower(s: &str) -> String {
    s.chars().map(lower_char).collect()
}

/// `s` with every character mapped by [`upper_char`].
#[must_use]
pub fn to_upper(s: &str) -> String {
    s.chars().map(upper_char).collect()
}

/// `s` in Unicode normalisation form C (`e` + U+0301 → `é`), borrowed when it already is.
#[must_use]
pub fn nfc(s: &str) -> Cow<'_, str> {
    if unicode_normalization::is_nfc(s) {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.nfc().collect())
    }
}

/// Removes non-spacing marks (`Mn`) after canonical decomposition and recomposes: `é` → `e`,
/// and Thai vowel and tone marks are dropped.
#[must_use]
pub fn remove_accents(s: &str) -> String {
    s.nfd()
        .filter(|c| c.general_category() != GeneralCategory::NonspacingMark)
        .nfc()
        .collect()
}
