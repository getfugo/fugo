# ssg-base

The shared vocabulary of the rewrite (docs/rust-port/REWRITE_PLAN.md §2.4). Every other crate
depends on it; it depends only on light crates (serde, serde_json, toml, serde-saphyr, jiff,
tera for `to_tera`, regex, unicode-properties, unicode-normalization, thiserror).

| module | contents |
|---|---|
| `id` | `Idx`, `define_id!`, `IdVec<I, T>`; `PageId`, `LangIdx`, `FormatId`, `MediaTypeId`, `ResourceId`, `TaxonomyIdx`, `TermIdx`, `ImageOpId`, `FrameId`, `TxnId` |
| `kind` | `PageKind` (serde names `home` … `404`, `robotstxt`), `KindSet` |
| `value` | `Value`, case-preserving byte-ordered `Map`, `Date` (`Zoned`/`Local`); `from_json[_str]`, `from_toml[_str]`, `from_yaml_str`, `Deserialize`, `Serialize`, `to_tera` |
| `params` | `Params::fold` (lower-case keys, not inside arrays), `get`, `get_path`, `insert`, `fill_missing_from`, `merge_deep` |
| `paths` | `ContentKey`, `TermKey`, `OutputPath`, `UrlPath`, `Permalink`; `sanitize` (Go's `MakePath`), `make_title`, `normalize_key`, `clean`, `join`, `split`, `base`, `parent`, `dir`, `ext`, `file_and_ext`, `filename`, `trim_ext`, `prettify_url_path`, `prettify_url`, `uglify` |
| `url` | `UrlRef` (parse/serialise with Go's escaping rules), `escape`/`unescape` per `Component`, `url_escape`, `path_escape`, `is_abs_url`, `make_permalink`, `add_context_root`, `BaseUrl`, `SiteUrls` (`make_path[_sanitized]`, `urlize`, `abs[_lang]_url`, `rel[_lang]_url`, `prepend_base_path[_abs]`, `permalink_for_base_url`) with `LinkStyle`/`PathCase`/`Accents` |
| `anchor` | `anchorize(s, Style::{Github, GithubAscii, Blackfriday})`, `Deduper` (`-1`, `-2` suffixes) |
| `inflect` | `pluralize`, `singularize`, `humanize`, `humanize_text` (template `humanize`), `titleize`, `ordinalize(i64)`, `ordinalize_str`; `Inflector` (+ `CustomInflections`) |
| `title` | `title_case(s, Style::{Ap, Chicago, Go, FirstUpper, None})`, `Style::parse` |
| `glob` | `compile(pattern, GlobOpts { case, separator }) -> Glob` (gobwas syntax, as the Go implementation uses it) |
| `time` | `parse_date(s, &TimeZone)` (the Go implementation's 21 date layouts), `Clock`, `DateError` |
| `diag` | `Position`, `Severity`, `Diagnostic`, `Diagnostics` (thread-safe, ignoreLogs, sorted + de-duplicated report) |
| `text` | general-category predicates, simple case mappings, `remove_accents`, `nfc` |
| crate root | `Collate` (+ `ByteOrder`), `Sink` |

## Oracle acceptance (T10)

`cargo test -p ssg-base -- --nocapture` prints the tallies. "Not applicable" counts cases
the Rust API cannot express (Go strings that are not UTF-8, argument values outside the Rust
parameter type, configurations the Go implementation never uses); they are not checks.

| fixture | checks | passed | accepted deviations | not applicable |
|---|---|---|---|---|
| `common/paths/strings` (18 of its 38 functions, see below) | 56,757 | 56,757 | 0 | 7 |
| `common/urls/baseurl` (new, WithProtocol, WithPort) | 3,764 | 3,762 | 2 | 97 |
| `helpers/pathspec` (13 URL functions × 14 setups) | 939,159 | 939,159 | 0 | 0 |
| `helpers/pathspec` `SanitizeAnchorName` | 5,788 | 5,788 | 0 | 0 |
| `common/flect` corpus (6 functions) | 19,320 | 19,294 | 26 | 7 |
| `common/flect` custom data (21 configurations) | 2,616 | 2,616 | 0 | 0 |
| `common/flect/upstream_tables.rs` (flect's own tests) | all | all | 0 | 0 |
| `common/prose` corpus (AP, Chicago, CreateTitle, AP∘pluralize) + upstream table | 13,201 | 13,201 | 0 | 7 |
| `common/cast` date strings (UTC, fixed +07:00, America/New_York) | 1,584 | 1,582 | 2 | 416 |
| `common/glob/compile` | 1,406 | 1,405 | 1 | 1 |
| `common/glob/match` (case-folded `GetGlob` mode + raw `""`/`"/"` separators) | 915,178 | 913,208 | 1,970 | 179,248 |

Out of scope here (owned elsewhere): `common/paths/pathparser` (vfs `PathParser`, T20),
`common/glob/filter` (mount include/exclude filters, T20), `common/locales` (T12), the non-date
conversions of `common/cast` (funcs, T31), `ResolveMarkup`/`TrimShortHTML` of
`helpers/pathspec` (page/markup). Of `common/paths/strings`, the Go helper functions without a
counterpart in this API are not tested: `AbsPathify`, `Add{Leading,Trailing,LeadingAndTrailing}Slash`,
`CommonDirPath`, `FieldsSlash`, `GetRelativePath*`, `HasExt`, `IsSameFilePath`,
`ReplaceExtension`, `ToSlash*`, `Trim{Leading,Trailing}`, `UrlFromFilename`,
`UrlStringToFilename` (OS-path plumbing or one-liners over `str`).

Errors are compared by kind (`Ok` against `Err`); Go's error texts are never reproduced (§1.2).

## Inflection and title case: crates evaluated

The plan asks to evaluate `pluralizer`, `cruet` and `titlecase` first. None of them is in the
offline registry of this workspace (network use was not approved for T10), so they could not be
run against the oracle; from their documented rules none can reach 100%: `pluralizer` (a port
of pluralize.js) and `cruet` (Rails' inflector) have other irregular/uncountable lists and
suffix rules than flect, and neither splits words or upper-cases acronyms the way flect does
(`humanize("api_key")` is `"API key"` in Go); `titlecase` implements John Gruber's rules,
not AP/Chicago as jdkato/prose implements them. So `inflect` and `title` carry their own data
tables: flect v1.0.3's dictionary, suffix rules and acronym list, and prose v1.2.1's small-word
and preposition lists (both MIT; see *Provenance* below). The algorithms are written from the
rules, not transliterated.

## Accepted deviations

- **YAML `.inf` / `-.inf` / `.nan`** decode to those strings, not to non-finite floats: an untyped
  serde-saphyr value cannot carry them, and rejecting them (the crate default) failed the whole
  document. The Go implementation (yaml.v2) gives float ±Inf/NaN.
1. **Strings that are not UTF-8** (not applicable): the API takes `&str`. Affects 7 inputs of
   each string corpus, 44 of the 274 glob inputs and one glob pattern.
2. **`BaseUrl`**
   - `http://example.com/%ff`: the decoded path is not UTF-8; `BaseUrl::parse` returns
     `UrlError::NonUtf8Path` instead of a base URL with a byte path.
   - `http://user@example.com:99999999999999999999/`: `BaseUrl::port()` is `Option<u16>` and
     returns `None`; Go saturates the port to `i64::MAX`.
   - `with_port` takes a `u16`, so the oracle's ports `-1` and `65536` are not applicable.
3. **`humanize` of text without words** (`"-"`, `"_"`, `"+"`, a lone combining mark; 13
   inputs × `humanize` and template `humanize`): Go fails with an index panic; we return the
   input (template `humanize`: the result of the steps that succeed).
4. **Custom inflections** (`inflections.json`, `acronyms.json`): loading errors are `Result`s
   (`InflectError`) instead of messages printed to stdout, and a custom word that repeats a
   built-in one is an error instead of a panic. Loading the files from the working directory is
   left to the caller.
5. **Dates**: `9999-12-31` in UTC and New York is after jiff's last instant
   (`9999-12-30T22:00Z`) and is rejected (2 checks). Typed Go strings (`template.HTML` …) and
   non-string inputs are the value layer's concern (not applicable).
6. **Globs**: the patterns in `expected_diffs.toml` `[glob]` (102 patterns, 1,970 checks; 673
   of the 304,980 `GetGlob`-mode checks) hit bugs of gobwas/glob's optimiser: an empty alternative
   group (`{}`, `{,}`, a trailing `{`) makes it match nothing; alternatives of different shapes
   (`{a*,b?}`) next to other wildcards are mis-compiled; `?` and `[!x]` match the empty string;
   fixed-width rows are measured in bytes (`[a-c]é` fails on `aé`); two negated classes in a row
   match anything. We implement the documented semantics instead. gobwas also rejects a pattern
   containing U+FFFD (it cannot tell it from a decoding error); here it is an ordinary
   character. Separator sets other than none and `/` (the oracle's `.`, `/.`, `é`) are not
   supported: the Go implementation only uses `/`.

## Design notes

- **Globs use `regex`, not `globset`.** The plan names globset, but globset matches bytes
  (`(?-u)`): `?` matches one byte and classes of non-ASCII characters are wrong (`[ก-ฮ]*`
  matched `é`), which matters for Thai paths; it also cannot express Go's `**` inside a
  segment (`**.json`) or nested alternatives. The pattern is parsed into a small syntax tree and
  compiled to an anchored Unicode regex (`regex` is already a workspace dependency).
- **Dates are parsed by a typed layout table, not jiff `strptime`.** strptime accepts
  one-digit months and days where Go's layouts require two, skips leading spaces, cannot parse
  zone abbreviations, and needs care for 12-hour clocks. jiff still builds and zones the result;
  gaps and folds resolve to the earlier instant (Go's result).
- **URLs** follow Go's escaping: components are kept decoded and re-escaped per component; an
  escaping seen at parse time is reused while it still decodes to the component (so `%2F` in a
  base URL survives joins).
- **Title case (AP/Chicago)** locates each word in a copy of the input with typographic
  punctuation replaced by ASCII and advances by the word's character count. Both belong to the
  rules as the Go implementation applies them (they decide which small words count as first or
  last); they are kept, which is why the prose oracle is at 100% including curly-quote titles.
- **`Params` collisions** (`Title` and `title` in one map): a key that was not lower case wins
  over one that was; among several, the last in byte order. Go's result depends on its map
  order there; this is the deterministic rule.

## Provenance

- `src/inflect.rs` tables: gobuffalo/flect v1.0.3 (`plural_rules.go`, `acronyms.go`), MIT,
  Copyright (c) 2019 Mark Bates.
- `src/title.rs` word lists: jdkato/prose v1.2.1 (`transform/title.go`), MIT, Copyright (c)
  2017 Joseph Kato.

These are data (word lists), rewritten into Rust tables; rows for `PROVENANCE.md` and the
licence texts for `THIRD_PARTY/` are to be added by the owner of those files.
