# ssg-locale

Everything language-dependent, on ICU4X 2.3 compiled data (CLDR 48.2.1, Unicode-3.0, see
`THIRD_PARTY/cldr/`). Depends only on `ssg-base` (REWRITE_PLAN §2.3).

## API

| Item | What |
|---|---|
| `Locale::new(key)` | One site language: collation (`impl base::Collate`), plural rules, number format, month/weekday names and the four date styles. Built once per language, `Send + Sync`. |
| `Collator::for_language(key)` | The collation alone (`impl base::Collate`). |
| `PluralRules::for_language(key)`, `PluralCount`, `PluralForm` | CLDR cardinal rules; a count keeps its decimal text (`1` is `one` in English, `1.0` is `other`). `PluralCount::from_value` is the Go implementation's count extraction (int, float, numeric string, a map's `Count` key). |
| `TranslationsBuilder::new(default_lang).missing_placeholders(b)` → `add_file(path, content)` … → `build(languages)` | Loads i18n files in precedence order (themes first); a later message replaces an earlier one. |
| `Translations::lookup(lang, key, &Args)` → `Translation::{Found, Fallback, Missing}` | Lookup through the language, its parent tags (`pt-br` → `pt`), then the default language. |
| `Translations::translate(lang, key, &Args)` | The Go implementation's `i18n`: the text, the default language's text, or `""`; `[i18n] key` for the latter two with `enableMissingTranslationPlaceholders`. |
| `Args { count, data }`, `Args::from_value(&Value)` | The Tera call `i18n(key=, count=?, data=?)`, or the Go implementation's single argument. |
| `MessageFile::read(path, content)` | The messages of one file (TOML/YAML/JSON; flat, `[key]` tables, nested namespaces, `[{id, translation}]` lists). |
| `Template::parse` / `parse_with(left, right)` / `render` | The restricted evaluator. |
| `format_number(n, precision, &Locale)` | `lang.FormatNumber`. |
| `format_date(&Zoned, DatePattern::{Strftime(fmt), Style(DateStyle)}, &Locale)` | `date(format=…, locale=…)` with localized `%B %b %h %A %a` (without `locale`, `date(format=)` prints English names as Go's `Time.Format`), and `:date_short` … `:date_full`. |

`Translations::load(vfs, langs)` of the plan's §2.4 sketch became the builder: this crate cannot
depend on `vfs` or `config` (§2.3), so `site` walks the i18n mounts and feeds the files, and
passes the language keys in `LangIdx` order. `format_number`/`format_date` take the `Locale`
instead of `config::Language` for the same reason.

## Decisions

- **Messages are not templates.** A message is text plus `{{ . }}` and `{{ .Field.Path }}`
  actions with optional `{{-`/`-}}` trim markers and the message's own `leftDelim`/`rightDelim`.
  Anything else (`if`, `with`, `printf`, pipes, variables, comments) is an `I18nError::Message`
  at load time naming the file, the key, the plural form and the action.
- **Values.** A missing map key or missing argument prints `<no value>` (as Go does). Floats
  print as the Go implementation's templates print them (`0.5`, `2`, `1e+06`). A map or list
  printed whole is an evaluation error (the Go implementation prints Go's `map[k:v]` syntax). A
  numeric argument is its own `.Count`; with only `count=` given, `.` and `.Count` are the count.
- **Errors at translate time** (`TranslateError`): the selected form and `other` both missing, or
  an argument that does not fit (`.Field` on a number). The caller logs a warning and renders
  nothing, like Go.
- **Plural rules** are ICU4X's CLDR 48 rules for the language key. A key CLDR does not know
  (`klingon`, `x1`) gets the root rules (`other` for everything); Go used the English rules.
- **Collation** is the language's CLDR tailoring (tertiary strength, non-ignorable), except Thai,
  which uses the root order: CLDR ≥ 24 gave Thai `[reorder Thai]` + `alternate=shifted`, which
  would sort a Thai site's mixed lists Thai-first and ignore punctuation; Go (x/text, CLDR 23)
  sorts them like the root order, and ICU4X root reproduces x/text on every sortable string of the
  reference sites (spec i18n-lang-misc §4), with one change for Thai: PAIYANNOI (ฯ) sorts as the
  punctuation mark `!` (before digits and letters), as in Go's Thai collation. ICU4X has no
  runtime tailoring rules, so the Thai collator compares with ฯ replaced by `!` (strings equal
  that way fall back to the plain order). Accepted deviation D5 (newer CLDR) otherwise.
- **Dates** always use the Gregorian calendar (`th` is `th-u-ca-gregory`), names in the format
  context (`MMMM`, `MMM`, `EEEE`, `EEE`). A language without CLDR date data formats as English.
- **Numbers**: rounded half-to-even on the binary value (`{:.*}`), then ICU grouping and symbols;
  `-0` prints as `0`.

## tera-contrib `date`

Checked: tera-contrib 0.3's `date(locale=, format=)` formats through
`FixedCalendarDateTimeNames<Gregorian>`, so it does produce Gregorian Thai month names. It is still
**not** the fugo `date` filter: with a `locale` it accepts only UTS-35 patterns and rejects
strftime (`%B`), it has no `style=` (`:date_long`), and it cannot default the locale to the
current page language (site-bound). fugo registers its own `date` on `format_date`
(`funcs`/`sitefuncs`, T31/T35), and the `date` feature of tera-contrib can be dropped from the
workspace (it pulls `jiff-icu`, `icu_calendar`, `icu_time` into `funcs` for nothing).

## Acceptance evidence (`cargo test -p ssg-locale`)

| Test | Result |
|---|---|
| `translate::translate_oracle` (61 sites, 3,378 calls) | 2,524 compared: 2,262 agree (89.6%); the other 262 are 4 classified deliberate deviations (print-collection 138, scalar-count-field 91, exponent-count 10, bool-count 6) and 17 listed calls; 0 unlisted. Not compared: 252 Go panics, 362 Go-only argument types (structs, `template.HTML`, `time.Month`), 240 calls of 3 messages with unsupported syntax. |
| `translate::go_load_errors_are_load_errors` | all 7 Go load failures fail here too, naming the file |
| `messages::*` | R's `welcome`/`reviews`/`comments` in en and th; unsupported syntax errors with file and key |
| `parse::message_file_layouts_oracle` | 148/161 files as go-i18n; 13 listed |
| `plural::plural_rules_oracle` | 99.53% on the 138 locales with ICU data; 11 CLDR rule changes and 74 locales without ICU data listed |
| `collate::*` | 2,801 site strings: consistent total order, no distinct strings equal, `th` = `en` = root, Latin before Thai, Thai leading vowels skipped, Thai ฯ as punctuation |
| `locales::*` | month/weekday names en and th (Gregorian) equal the Go implementation's locales library except `th` abbreviated weekdays; date styles 701/701 except listed fields; numbers 845/845 |

Every difference is listed with its reason in [`expected_diffs.toml`](expected_diffs.toml); the
tests fail on unlisted differences and on stale entries.

## Workspace note

ICU4X data payloads are `Rc`-based unless `icu_provider/sync` is on; `Locale` and
`Translations` must be `Send + Sync` (`base::Collate`, `Arc` sharing under rayon). T12 added
`"sync"` to the `icu_provider` entry of the workspace-hack block in `Cargo.toml` (no new
crate).
