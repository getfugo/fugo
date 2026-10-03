# ssg-pageparser

Content files (docs/rust-port/REWRITE_PLAN.md §2.1, §2.4, §3.2, task T11): front matter split
and decode, the summary divider, and the shortcode lexer (`lex` + `assemble`). Depends on
`ssg-base` (`Value`, `Map`, `Params`, general categories), `memchr` and `thiserror`.

| item | what it does |
|---|---|
| `split_front_matter(&str) -> Result<Split, LexError>` | skips blank lines and byte order marks; `---` YAML, `+++` TOML (CRLF allowed), a `{…}` JSON object, `#+` Org lines; `Split { front_matter: Option<(FrontMatterFormat, &str)>, body, body_offset }` |
| `decode_front_matter(format, text) -> Result<Params, DecodeError>` | through `base::Value::from_{yaml,toml,json}_str`, then `Params::fold`; an empty or `null` document is empty; Org is `DecodeError::Unsupported` |
| `decode_front_matter_map` | the same, case preserved (`Map`) |
| `lex(&str) -> Result<Vec<Token>, LexError>` | a body; `Token { kind: TokenKind, span }` |
| `lex_with(&[u8], LexOptions) -> Lexed` | any bytes, from the page start (`Start::Page`) or the body, with `SummaryDivider::{Html, Org, Off}`; keeps the tokens before an error |
| `assemble(src, &[Token], &dyn InnerOracle) -> Result<Body, ParseError>` | text and `ShortcodeCall`s (`name`, `delim`, `inline`, `args`, `closing`, `indentation`, `ordinal`, `span`); `Body.summary_divider` is the index of the first segment after the divider |
| `parse_body(&str, &dyn InnerOracle)` | `lex` + `assemble` |
| `line_col(src, offset)` | 1-based line and column for diagnostics |

Token kinds: `ByteOrderMark`, `FrontMatter(format)`, `SummaryDivider`, `Text`, `Indentation`,
`LeftDelim(Delim)`, `RightDelim(Delim)`, `Close`, `Name`, `InlineName`, `Param(Quoting)`,
`Value(Quoting)`. `Token::value` removes the backslashes of an escaped string;
`Token::scalar` types a bare argument (`true`/`false`, `[+-]digits` → `i64`,
`[+-]digits.digits` → `f64`, else a string; `0.125.0` and `1e3` stay strings).

`InnerUse` (`Required`, `Unused`, `UnknownShortcode`) decides where a call ends, as in Go:
a shortcode that uses `inner` collects its inner content up to its closing tag and must be
closed or self-closed; one that does not ends at its opening tag's `>}}`, and a closing tag or
`/>` for it is an error; an unknown shortcode is an error. Inline shortcodes (`name.inline`)
always take inner content. `ordinal` counts per nesting level.

## Oracle acceptance (T11)

`cargo test -p ssg-pageparser -- --nocapture` prints the tallies. The fixture is
`testdata/oracle/parser/pageparser/pages.json.gz` (the Go implementation's `ParseBytes` in
three configurations and `ParseFrontMatterAndContent` over docs, testsite, skeletons, the
lexer's own test strings, hand-written shapes and 4,000 random soups).

| check | result |
|---|---|
| lexer items, kinds + byte ranges (+ `isString`, escape segments), 3 configurations × 5,322 inputs, errors and EOF included | **135,326 / 135,326** (plus 15,966 item-count checks) |
| typed arguments (`ValTyped` of params and values) | 9,206 / 9,206 |
| `split_front_matter`: format and body offset | 4,288 / 4,288 (1,034 inputs not UTF-8: not applicable) |
| `lex(body)` after the split = page lexer after the front matter | 3,975 + 23 accepted (rule `divider_at_start`) / 3,998 |
| front matter decode (docs, tests, shapes, soups) | 2,034 + 31 accepted / 2,065 (Org and non-UTF-8: not applicable) |
| `assemble` of every docs/testsite/skeleton body that lexes | 980 / 980 files, 804 calls |

The lexer works on bytes (Go decodes invalid UTF-8 as U+FFFD, one byte each; so does ours),
so non-UTF-8 inputs are checked too. Error texts are our own (§1.2); only error kinds and
ranges are compared.

## Accepted deviations

Reviewed in `expected_diffs.toml`; each is checked by the test, by rule or by case id.

1. **Summary divider at the start of a page without front matter** (`divider_at_start`, 23
   inputs): the body is lexed after `split_front_matter`, so `<!--more-->` as the first
   non-blank text is a divider. Go's page lexer has consumed its `<` while looking for front
   matter and reads it as text — a state-machine artefact (§1.2). The full-page lexer
   (`lex_with(.., Start::Page)`) still matches Go byte for byte.
2. **JSON numbers** (`json_numbers`, 26 inputs): integers stay `Value::Int`; Go decodes every
   JSON number as `float64`.
3. **YAML 1.2** (D5; `shape#0`, `shape#3`): `yes` stays a string, `017` is decimal, integers
   beyond `i64` become floats, and `.inf`/`.nan` are rejected by `base::Value::from_yaml_str`
   (serde-saphyr's default `reject_non_finite_typeless_float`), which fails that document.
   `: bad` is a map with the empty key (`shape#21`); yaml.v2 rejects it.
4. **TOML** (`shape#1`, `shape#4`): go-toml parses `6.626e-34` one ulp off; a leap second
   (`23:59:60`) stays a string (Go normalises it to the next minute).
5. **Stricter closing tags**: a closing tag must name the shortcode it closes. The Go lexer
   only checks that the name was opened somewhere, so `{{< a >}}{{< b >}}{{< /a >}}{{< /a >}}`
   (both using `inner`) closes `b` with `/a` in Go; here it is `MismatchedClose`.
6. **Unclosed inline shortcodes** are `ParseError::Unclosed`; Go swallows the rest of the
   page into the inline template.
7. **Org front matter** is lexed (with the `# more` divider) but not decoded (COULD).

Go quirks that *are* kept, because they decide which pages build: a page starting with `-`,
`+` or `{` must hold valid front matter (`- item` as the first line is an error); a shortcode
closing tag or `/>` for a shortcode that does not use `inner` is an error; a summary divider
inside a shortcode's inner content is dropped; the first `<!--more-->` only is a divider.

## Plan deviations (API)

- `Token` has no lifetime: it is a kind and a byte range; `Token::text`/`value`/`scalar` take
  the source. This lets the lexer run over bytes that are not UTF-8 (the oracle has 1,034 such
  inputs), and `assemble` takes the source next to the tokens.
- `Segment::Escaped` is not needed: an escaped shortcode `{{</* x */>}}` is three text segments
  (`{{<`, ` x `, `>}}`), exactly the Go implementation's items.
- `ShortcodeCall` has a byte `span` and no `Position`: this crate does not know the file; the
  caller builds a `base::diag::Position` with `line_col`.
- `Closing::Closed` also carries the inner content's source range; `ShortcodeCall` has
  `inline`, `indentation` and `ordinal`.
- `split_front_matter` and `lex` return `LexError`; `assemble` returns `ParseError` (which
  wraps `LexError`).

## Provenance

Written from the Go implementation's `parser/pageparser` behaviour (Apache-2.0) and checked
against its oracle; no code copied.
