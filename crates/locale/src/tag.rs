//! Language keys (`en`, `pt-br`, `zh-Hant-TW`, `klingon`) and their ICU locales.

use icu_locale_core::Locale as IcuLocale;

/// The key a language is known by: lower case, `_` written as `-` (`zh_TW` → `zh-tw`).
pub(crate) fn normalize(key: &str) -> String {
    key.trim().to_ascii_lowercase().replace('_', "-")
}

/// The ICU locale of a language key, or `None` when the key is not a BCP 47 tag (`x1`).
///
/// A key such as `klingon` is a syntactically valid language subtag without CLDR data; ICU then
/// falls back to the root locale for everything.
pub(crate) fn icu_locale(key: &str) -> Option<IcuLocale> {
    IcuLocale::try_from_str(&normalize(key)).ok()
}

/// The ICU locale for formatting in language `key`: the key's locale when CLDR has data for it
/// (beyond root), English otherwise (`klingon`, `x1`), as Go does.
pub(crate) fn cldr_locale(key: &str) -> IcuLocale {
    icu_locale(key)
        .filter(has_cldr_data)
        .unwrap_or(icu_locale_core::locale!("en"))
}

/// Whether CLDR knows the language: it has its own month names (the root locale's are
/// `M01`…`M12`). Plural or collation data cannot tell, since languages whose rules equal the
/// root's (Thai, Japanese) have none of their own.
fn has_cldr_data(locale: &IcuLocale) -> bool {
    use icu_datetime::fieldsets::M;
    use icu_datetime::input::Date;
    use icu_datetime::preferences::CalendarAlgorithm;
    use icu_datetime::{DateTimeFormatter, DateTimeFormatterPreferences};

    let mut prefs = DateTimeFormatterPreferences::from(locale);
    prefs.calendar_algorithm = Some(CalendarAlgorithm::Gregory);
    let Ok(formatter) = DateTimeFormatter::<M>::try_new(prefs, M::long()) else {
        return false;
    };
    let january = Date::try_new_iso(2021, 1, 1).expect("valid date");
    formatter.format(&january).to_string() != "M01"
}

/// The parent keys of a language key, most specific first: `zh-hant-tw` → `zh-hant`, `zh`.
pub(crate) fn parents(key: &str) -> impl Iterator<Item = &str> {
    let mut rest = key;
    std::iter::from_fn(move || {
        let cut = rest.rfind('-')?;
        rest = &rest[..cut];
        Some(rest)
    })
    .filter(|k| !k.is_empty())
}
