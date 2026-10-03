# ssg-publish

Sinks, canonify, minify dispatch, URL-token extraction, held outputs and the static sync
(REWRITE_PLAN.md §2.6, §3.4; phases E1, E2 output and the `patch_held` half of E5).

| API | What |
|---|---|
| `DiskSink { root }`, `MemorySink { files: DashMap<OutputPath, Arc<[u8]>> }` | `base::Sink` (`write`, `exists`, `read`); the disk sink creates parent directories and truncates; the memory sink has `get`, `text`, `paths` (sorted) and `write_to(dir)` |
| `PublishSettings::from_config(&Config)` | per-language `SiteLinks` (base URL, `canonifyURLs`, `relativeURLs`, LiveReload URL: `None` here, set by `ssg-build` for `serve`), output formats and media types, the `Minifier` when `minifyOutput` is set |
| `Publisher::new(settings, Arc<dyn Sink>, Arc<Diagnostics>)` | shared by the render workers (`Send + Sync`) |
| `Publisher::with_css_purges(Arc<CssPurges>)`, `page_names(html)` | `purge_css` placeholders (`__nh_purge_<n>__`) are replaced first in `emit` (and in `patch_held`) by the CSS the output uses: `page_names` gives the tags, classes and ids of its elements, the words of its `<script>` elements and every `--name` it mentions |
| `Publisher::emit(Output { path, text, format, lang, alias })` | `purge_css` placeholders → canonify / relative URLs (RSS always, HTML when configured) → LiveReload script (HTML outputs of a language with a LiveReload URL, not aliases, as in Go; `serve`) → URL tokens → hold when a `__nh_defer_` / `__nh_pp_` placeholder is present (the text is written to the sink unpatched and unminified, only the path is kept), else minify by media type and write. Empty text writes nothing (`Emitted::Empty`). A minifier error writes the output unminified with a `minify-output` warning |
| `Publisher::patch_held(&BTreeMap<placeholder, text>)` | reads every held output back from the sink (`PublishError::Read` if it is gone), replaces its placeholders, rewrites its URLs again (canonify / relative), extracts its URL tokens again, minifies and writes (rayon, outside renders); a placeholder left over is `PublishError::UnresolvedPlaceholder` |
| `Publisher::add_tokens_from(text)`, `url_tokens()` | `execute_as_template` results; the sorted `UrlTokens` so far |
| `UrlRewriter::{absolute, relative, new}.rewrite(bytes, Quoting)` | the canonify rewriter (also `rewrite_str`); `canonify::dotted_path_to_root` |
| `HtmlElements::collect(html)` | the tags, classes and ids of HTML (what `page_names` starts from) |
| `UrlTokens::{extract, iter, contains}` | URL-shaped words after decoding HTML references and JSON escapes (srcset lists, unquoted attributes, `./` / `../` resolved against the output) |
| `sync_static_dir(&Vfs, root, &StaticSyncOptions)` | phase E1 on disk: rewrite only changed files, copy permissions and modification times (`noChmod`, `noTimes`), `cleanDestinationDir` (keeps `.`-directories; on macOS the names it lists are NFC-normalised before they are compared with the static files'), multihost language directories; returns the file count |
| `sync_static(&Vfs, &dyn Sink, &StaticSyncOptions)` | the same files into any sink (memory builds) |
| `StaticSyncOptions::target(&FileRef)` | the publish path of a static file (below its language directory on a multihost site); `serve` copies single changed files with it |
| `livereload::{script, inject}` | the LiveReload `<script>` placed at the start of the head |

**URL-token seam.** `ssg-resources` does not depend on this crate (§2.3). The store's
`publish` takes any `IntoIterator<Item = &str>`; `ssg-build` passes `publisher.url_tokens().iter()`
(or `&UrlTokens`, which is `IntoIterator<Item = &str>`). Tokens are decoded but not
canonicalised: the store reduces them (percent-decoding, host, query) to its own URL index.

## Rules

- **Canonify** (`canonify.rs`): a candidate follows `src=`, `href=`, `url=`, `action=` or
  `srcset=` (case-sensitive substrings); after the first four an optional quote and a
  root-relative URL (`/x`, not `//x`), after `srcset=` a quoted list starting with one (at most
  2000 bytes up to the closing quote; every root-relative candidate is rewritten and white
  space collapses to single spaces). The prefix's own path (`docs/`) is dropped when the URL
  repeats it. HTML quotes are `"` `'`, RSS quotes `&#34;` `&#39;`. Aliases are HTML outputs.
- **HTML elements** (`elements.rs`, for `purge_css`): html5gum tokens; every start tag (lower-cased), `id` values,
  `class` and `*transition*` attribute words, and Vue/Alpine `:class` bindings (object keys, or
  single-quoted words). The content of `pre`, `textarea`, `script` and `style` is skipped.
  Scanned before minification.
- **Held outputs**: the plan's placeholder prefixes (`PLACEHOLDER_PREFIXES`). A held output waits
  in the sink at its own path, as Go's post-processing writes its files before patching them, so
  held pages cost no memory (on a site where every page is held: 386 MB → 234 MB). Replacement
  text is inserted into the canonified output, the result is rewritten once more (T36: the links
  of a pending `fingerprint` are post-process placeholders until E5, and Go canonifies them; the
  rewrite leaves already rewritten URLs alone), then minified with the page.

## Acceptance evidence

`cargo test -p ssg-publish` (lib 7, it 19):

- **canonify** — `oracle/transform/absurl/cases.jsonl.gz`: 94,180 cases, 93,178 exact, 1,002
  accepted in 3 Go-quirk classes (below), 0 unexplained; the upstream `absurlreplacer_test.go`
  tables exact.
- **LiveReload** — `oracle/transform/absurl/inject.jsonl.gz`: 175/175 livereload records
  exact; 14 generator-tag records accepted (never injected), 11 equal.
- **HTML elements** — `oracle/publisher/collector/collector.jsonl.gz` (Go's collector, which
  wrote the Go build's stats file; this port writes none, `HtmlElements` feeds `purge_css`): the upstream
  `TestClassCollector` documents 170/170 exact; html5lib documents 1,662/1,708; hand-written
  documents 185/265; element strings 28,673/36,795; element documents 8,527/12,265; random
  documents 2,288/4,000; multi-write streams 765/1,260; groups 7/25 (56,488 checks, every
  difference classified in `expected_diffs.toml`, 0 unexplained). The 3,022 `closed` records test Go's private `isClosedByTag`
  and have no counterpart.
- **static sync** — `oracle/commands/staticcopy/staticcopy.json.gz`: 12 cases, 179 checks
  (count + every entry's bytes, mode, mtime), 171 exact, 8 accepted, 0 unexplained (later
  static mounts of a module win and symbolic links are followed, both in `Vfs::walk`);
  `sync_static` into a `MemorySink` (plain and multihost).
- **publisher** — canonify per format (HTML with `canonifyURLs`, RSS always with entity quotes,
  JSON never), `relativeURLs`, minify dispatch (HTML, JSON, `text/plain` untouched, invalid
  JSON published with a warning), empty outputs, held outputs patched then re-scanned (the
  PostProcess URL becomes a token) and minified, unresolved placeholders, URL tokens (entities,
  JSON escapes, srcset, `&`/`'` paths), identical tokens and paths with 1 and 4 threads, disk and memory sinks, the LiveReload script per language (HTML pages
  only; not aliases, RSS, or a language without a LiveReload URL).

## Accepted deviations

All counted in `expected_diffs.toml` (a changed count fails the tests):

| Family | Classes |
|---|---|
| absurl | `go-panic` 93, `leading-candidate` 855 (Go's prefix positions start at 0, so a document starting with `/x` gets the base written up to 4 times), `prefix-inside-rewrite` 54 (stale positions jump back into written input) |
| inject | `generator-tag-never-injected` 14 |
| collector-* | x/net/html tree-builder and Go-scanner artefacts: markup declarations as tags, table/head/frame elements dropped in a body context, `<prefix>`-style raw-text skips, quotes tracked across tags, repeated attributes, Unicode tag names, NUL and numeric references, characters split across writes, invalid UTF-8 |
| staticcopy | `shadowed-files-counted` 1 (Go counts a path once per static mount of one module holding it; fugo counts published files), `empty-dirs-not-copied` 6, `missing-static-dir` 1 |

Not implemented here: `Output` carries no `JobOrder` (that type lives in `render`, which this
crate does not depend on; collisions are ordered by `build`).
