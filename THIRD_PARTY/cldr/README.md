# Unicode CLDR data (via ICU4X compiled data)

`ssg-locale` formats, collates and pluralizes with ICU4X 2.3 and its compiled data crates.
Those crates embed data generated from the Unicode Common Locale Data Repository (CLDR) and ICU;
they are ordinary crates in `Cargo.lock` (licence `Unicode-3.0`, allowed in `deny.toml`), and
this directory keeps the licence text with the fugo sources as well, since the data ends up in
the `fugo` binary.

| Crate (version) | Data used by fugo |
|---|---|
| `icu_collator_data` 2.3.0 | root collation and language tailorings |
| `icu_plurals_data` 2.3.0 | cardinal plural rules |
| `icu_decimal_data` 2.3.0 | decimal symbols and grouping (`format_number`) |
| `icu_datetime_data` 2.3.0, `icu_calendar_data` 2.3.0, `icu_time_data` 2.3.1 | date patterns, month and weekday names (Gregorian) |
| `icu_locale_data` 2.3.0, `icu_locale_fallback_data` 2.3.0 | locale fallback |
| `icu_normalizer_data` 2.3.0, `icu_properties_data` 2.3.0 | normalization and properties used by the collator |

Source: <https://github.com/unicode-org/icu4x>, data generated (per the crates' metadata) from
**CLDR 48.2.1** and ICU `release-78.1rc`. Upstream CLDR: <https://github.com/unicode-org/cldr>.

Licence: Unicode License v3 (`Unicode-3.0`), [`LICENSE`](LICENSE), copied verbatim from
`icu_plurals_data-2.3.0/LICENSE` (identical in every data crate above).

Added by T12 (ssg-locale).
