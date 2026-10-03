# ssg-page

Per-page rules (REWRITE_PLAN.md §2.4, phases B2 and B4): typed front matter, dates, build
policy, cascade, target paths and permalinks, the default sort order and default titles. The
crate holds no page store; `ssg-site` calls it while it assembles the model.

| Piece | API |
|---|---|
| Capture overrides | `capture_overrides(&Params) -> CaptureOverrides { kind, lang, path: Option<ContentKey> }` |
| Cascade | `Cascade::decode(&Value)`, `Cascade::from_config(&[CascadeConfig])`, `Cascade::inherit(parent, own)`, `Cascade::apply(&MatchCtx, &mut Params)`; `CascadeTarget::new(kind, path, lang, env)` (Go's globs via `base::glob`), `matches`, `path_looks_like_file` |
| Content adapters | `AdapterPage::decode(&Params, base: &ContentKey, &MediaTypes)` (an `add_page` map: kind as written, path (one leading `/` removed, joined to the adapter's directory, lower case, spaces → `-`, not trimmed), content media type and value, cascade; `source_path()` = `/<path>/index.<suffix>` or `_index`), `meta_from_adapter(&AdapterPage, fields, params, &MetaCtx) -> PageMeta`, `Cascade::apply_split`, `DateResolver::adapter_dates` (the given dates, then the four `[frontmatter]` chains in turn over them; Go's `pagesfromdata` and the `IsFromContentAdapter` paths of `pagemeta`) |
| Front matter | `meta_from_params(Params, &MetaCtx) -> PageMeta` (reserved keys typed, including the legacy `_build` (wins over `build`, as in Go) and the undocumented `published: <bool>` (`!draft` when `draft` is unset); normalised values written back to params; `params:` merged last; dates resolved) |
| Dates | `DateResolver::{new, from_site, resolve(&mut Params, Option<&FileCtx>, &TimeZone)} -> DateOutcome { dates, slug, unparsable }` |
| Build | `BuildPolicy { list: ListMode, render: RenderMode, publish_resources }`, `BuildPolicy::decode`, `.headless()` |
| Markup | `Markup::{Markdown, Html}`, `Markup::detect(MarkupSource { media_type, markup, ext }, &MediaTypes)` |
| Permalinks | `PermalinkPattern::{parse, expand(&PermalinkCtx)}`; `PermalinkPatterns::{compile(&config::Permalinks), get(kind, section)}`; Go layouts (`:2006`, `:Jan`, `:02`, `:MST`) become `strftime` at parse |
| Target paths | `target_paths(&UrlInputs) -> TargetPaths { target: OutputPath, link: UrlPath, resources: Option<ResourceBase> }`; `links(&TargetPaths, &SiteUrls, &OutputFormat) -> Links { rel_permalink: UrlPath, permalink: Permalink }`; `SourcePath::{from_path_info, from_key}` |
| Order | `default_order(&SortKey, &SortKey, &dyn Collate)` (ordinal, taxonomy weight, weight with 0 last, date desc to the second, collated link title, source path) |
| Titles | `default_title(kind, raw, &TitleConfig)` |

## Acceptance (tests/it)

The oracle tests replay `testdata/oracle/page/{paths,permalinks,frontmatter,misc,collections}`
(the per-page families; `menus`, `pagination` and `related` are `ssg-nav`'s, `summary` is
`ssg-markup`'s) and print pass rates. Every difference is either exact or a reviewed class
of `expected_diffs.toml` with its exact count:

| Family | Checks | Exact | Accepted |
|---|---|---|---|
| paths (target file, link, resource dirs, rel/permalink) | 80,299 | 99.66 % | 272 `url-query-or-escape` |
| permalinks (site config, every-token config, 55 patterns) | 89,148 | 100 % | – |
| permalinks config decode (`config::Permalinks::decode` + `PermalinkPatterns::compile`) | 36 | 97.2 % | 1 `go-typed-map` |
| frontmatter dates (recorded calls and 16,568 replays) | 18,398 | 99.62 % | 70 `unix-seconds-in-utc` |
| frontmatter config decode (`config::decode_front_matter`) | 14 | 100 % | – |
| build options | 15 | 100 % | – |
| cascade decode / match | 18 / 144 | 100 % / 87.5 % | 18 `bad-glob-rejected` |
| markup detection / page config (incl. 5 content-adapter configs) | 26 / 21 | 57.7 % / 95.2 % | 12 `markup-not-supported` |
| default sort (`SortByDefault`, every current site) | 617 | 100 % | – |
| source paths (`SourcePath::from_path_info` over the vfs parser) | 1,798 | 100 % | – |

Overall 190,534 checks, 99.80 % exact, no unexplained difference.

## Deviations from Go (by design)

- **Attribute expansion.** Go replaces permalink attributes one by one with
  `strings.Replace(…, 1)` in the partially expanded pattern; here each attribute is expanded in
  place. They differ only when a title or slug contains text such as `:slug` (no fixture case).
- **Unix-second dates** are UTC, not the process's local zone.
- **Cascade globs that do not compile** are errors (Go ignores or panics).
- **Only Markdown and HTML** content (also for content adapters' `content.mediaType`).
- **Content adapter maps** are decoded leniently where Go's `mapstructure.WeakDecode` would
  fail: `dates` also take date strings and Unix seconds besides date values (Tera has no time
  type: `to_date` gives `{rfc3339, unix}`, which is read); the messages are fugo's. A
  `kind` that is not one of `page`, `home`, `section`, `taxonomy`, `term` as written (Go
  does not fold its case or map `taxonomyTerm` here, unlike front matter) is an error: Go
  adds a page of that kind that has no output format and is listed only in `site.Pages`,
  `site.AllPages` and `GetPage`, which fugo's page kinds cannot hold.
  Otherwise Go's rules: no reserved keys or dates in `.Params`, no `_build`, `headless`,
  `published`, `menu` or `resources`, the `slug` as given (no `-` trimmed), sitemap settings
  from zero (an adapter page in the sitemap has `<priority>0</priority>`, as in Go; the view
  gives an integral priority as an integer), and the dates of
  `createContentAdapterDatesHandler`: the given dates, then the date, lastmod, publishDate and
  expiryDate chains in turn, each over the dates the earlier ones left (with the default
  `[frontmatter]`, `dates.lastmod` alone gives all three dates). Checked against the Go binary
  for 8 `[frontmatter]` configurations × 10 date maps.
- **Page links are URL paths.** `TargetPaths::link` is escaped when written (`%XX` escapes are
  kept, as Go's `EscapedPath` keeps them); a front matter `url` with a query gets `%3F`.
- **Resource output directories are clean paths** (`/th/section`, not `/th/section/`).
- **Standalone formats** (404, sitemap, robots.txt; no resource directory) are recognised by
  name, not by struct equality with the built-in formats.
- **Menus in front matter**: a list of menu names gives one entry per menu (Go shares one entry
  whose menu is the last name).
- **Date errors are data** (`DateOutcome::unparsable`), reported once per key by the caller.
