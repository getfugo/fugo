# ssg-build

Build orchestration (REWRITE_PLAN.md §2.6, §3). **State: T36** (the full §3 pipeline on T38's
entry point).

```rust
pub enum SinkKind { Disk /* default */, Memory }
pub struct BuildRequest { pub source: PathBuf, pub destination: Option<PathBuf>,
                          pub config_files: Vec<PathBuf> /* T37: --config */, pub cli: CliOverrides,
                          pub clock: Option<jiff::Timestamp>, pub sink: SinkKind,
                          pub clean_destination: Option<bool> /* None: the config's cleanDestinationDir */,
                          pub threads: Option<usize> /* T36: the render pool size */,
                          pub config: Option<Arc<Config>> /* T71: loaded by the caller (server) */,
                          pub live_reload: Option<LiveReload> /* T71: the server's script */,
                          pub server: bool,
                          pub prepare: Option<Arc<dyn Prepare>> /* run after loading the config, before the Vfs */ }
pub trait Prepare: Send + Sync + Debug { fn prepare(&self, cfg: &Config) -> Result<(), String>; }
pub struct LiveReload { pub port: Option<u16> /* --liveReloadPort */ }  // url(&BaseUrl) -> UrlRef
pub fn build(r: BuildRequest) -> Result<BuildReport, BuildError>;
pub fn process_env() -> Vec<(String, String)>; // T37: FUGO*, HOME, XDG_CACHE_HOME, TMPDIR, USER (the CLI's config loads too)
pub enum BuildError { Config, Vfs, Model, Template, Render, Publish, Resource, Pool, Prepare(String), Diagnostics(Vec<Diagnostic>) }
pub struct Collision { pub path: OutputPath, pub winner: JobOrder, pub loser: JobOrder }
pub struct BuildReport { pub pages, pub outputs, pub aliases, pub resources, pub images, pub static_files: usize,
                         pub collisions: Vec<Collision>, pub diagnostics: Vec<Diagnostic>,
                         pub timings: Vec<(&'static str, Duration)>, pub memory: Option<Arc<MemorySink>>,
                         pub model: Option<Arc<Model>> /* T71: the rendered model (file → page) */ }
```

**`fugo server` (T71).** The server loads the configuration itself (once per
configuration change) with every language's base URL pointed at its listener and passes it as
`config` (then `source`, `config_files`, `cli` and `destination` are not read). With
`live_reload`, every language's `SiteLinks::livereload` is its base URL (the port replaced by
`LiveReload::port` when set), so the publisher puts the LiveReload script into every HTML page
but the alias redirects (`Output::alias`, set for alias, `page/1/` alias and language-redirect
jobs). `build` never sets it (the A-T bytes are unchanged). `model` lets the server find the
page of a changed content file (`--navigateToChanged`).

**`prepare`.** A build that loads its configuration (`config` is `None`) calls
`Prepare::prepare` with it before it builds the `Vfs`. The CLI installs the project's npm
packages there (`ssg-npm`), so that mounts of `node_modules` exist when the `Vfs` is built. The
server calls it itself whenever it loads the configuration: at the start, and when the
configuration or `package.json` changes. A failure is `BuildError::Prepare`.

`Vfs` and `Pool` are beyond §2.6 (the Vfs has its own error type; the render pool may fail to
start).

## Phases (`src/lib.rs`, `src/waves.rs`, `src/deferred.rs`)

| Phase | What |
|---|---|
| A1–B6 | `config::load` → `Vfs::new` → `site::capture_content` → `site::assemble` |
| A4b | content adapters (`src/adapters.rs`, only when the content has `_content.html` files): the layouts are scanned first, a `Session` of the content files' model renders each adapter (`Session::render_adapter`) for its language, then for the other languages when it called `enable_all_languages()`, one after the other in discovery order; `site::assemble` again with the pages and resources they added; the diagnostics of their runs join the build's. A `_content.html` with Go-template syntax is an error with a hint (`ssg_layouts::go_marker`), a `_content.gotmpl` too (`site`) |
| B7, C0 | `LayoutStore::scan` → `Session::new` (site functions, i18n files, views, selections) |
| C1, D | `render_content` (every page, every hook variant) → `freeze_views` |
| E1 | static files: `sync_static_dir` on disk (`noTimes`, `noChmod`, `cleanDestinationDir`, multihost language directories), `sync_static` into memory |
| E2 | wave 1, **one sub-wave per language in language order** (`Session::wave1`) |
| E3 | wave 2 (`Session::wave2`): pagers 2..N and `page/1/` aliases of the paginations wave 1 recorded, the language redirect |
| E4 | — (the Go build wrote its stats file here; this port writes none) |
| E5 | every `defer(...)` key rendered once (`Session::render_deferred`, in parallel over keys; `data`, `site`, `build`, `__nh` in phase `Deferred`); every post-process placeholder resolved (pending transforms run now, after every page; placeholders inside deferred output too) → `Publisher::patch_held` (read back from the sink, rewrite again, URL tokens again, minify, write) |
| E6 | URL tokens of `execute_as_template` results added, then `ResourceStore::publish(tokens)`: eager bundle files, `publish`ed resources, every resource named by a token; processed images through the image queue (`[caches.images]`) |
| E7 | sorted, de-duplicated diagnostics; errors fail the build (`BuildError::Diagnostics`) — after every file was written, as Go's build does |

**Render pool.** Every parallel phase runs on one rayon pool (`stack_size(16 MiB)`, `threads`,
else `RAYON_NUM_THREADS`, else the CPUs); parallel work is started only from outside it
(`RenderPool::run` debug-asserts `current_thread_index().is_none()`).

**Collisions** (`waves.rs`). Every job's target is known before rendering
(`Session::target`). A page (or pager) beats an alias; among jobs of one class the later
`JobOrder` wins; a job whose target an earlier wave claimed is compared with that file's job
(later languages win among equals). Each collision is a `target-collision` warning and a
`Collision`. Losing jobs are still rendered and their outputs dropped: Go renders every page,
so a losing list page still records its pagination and its pagers exist (they compete for their
own targets in wave 2).

**Publishing.** Each winning output goes to `Publisher::emit` from its render worker; errors are
reported in job order.

## Tests (`cargo test -p ssg-build -- --nocapture`)

| Test | What | Result |
|---|---|---|
| `adapters::*` | Go's content adapter integration tests (`pagesfromdata/pagesfromgotmpl_integration_test.go`) with Tera adapters and layouts: pages and resources (text, an asset passed as content and resized, names, titles, params, mixed-case paths), drafts from data, errors at the call (`path`, `lang`, `content.markup`, cascade, media type), `site` without page lists, adapter functions outside adapters, Go-template adapters, one adapter per language, `enable_all_languages` with the shared store, the default sort, cascade (adapter section, content file onto adapter pages), build options, dots in paths, param case, paths joined as Go joins them (one leading `/` off a page path, none off a resource path, nothing trimmed), resource media type, menus, a summary divider, outputs, the home page, the docs' news shape (listed locally, not rendered, RSS), and what the Go binary gives for slugs kept as given, sitemap priorities printed as Go prints them (`0`, `1`), chained dates and a path added twice (the last call wins, with a warning), and a path two adapters add (the later adapter wins; a content file wins over both, as the Go binary with one collector worker); 20,000 pages and 20,000 resources from one adapter with repeats, a content file and a later adapter on some of their paths (the same rules; the model phase under 8 s in a test build: 1.5 s with the maps of paths, 13–16 s with scans) | **22/22** |
| `skeleton::testsite_{l1,bytes,contents}` | `sites.py make testsite` with `sites/testsite/layouts` vs Go's `public/` (`tests/it/testsite-go.txtar`) | **55/55 files, 55/55 byte-identical** |
| `skeleton::testsite_for_the_server` | the testsite with a caller-loaded configuration (base URL `http://localhost:1313/`) and `live_reload` | the script right after `<head>` (after `<html>` in the 404 page) of every HTML page, none in aliases, `page/1/`, the language redirect, RSS and JSON; canonified links on the server URL; `model` set |
| `mini::mini_matches_the_go_tree` | the e2e `mini.txtar` site (en/th, hooks, shortcodes, pagination, taxonomies, menus, i18n, data, related, aliases, `defer`, `GetRemote` from the file cache + `unmarshal`, minify, fingerprint, Concat, ExecuteAsTemplate, FromString, PostProcess), Go layouts converted to Tera in the test, `--minify --clock`, vs the Go tree of `e2e.json.gz` | **53/53 files** (fingerprints normalised); no placeholder left, deferred footer everywhere, post-processed CSS linked and published |
| `edges::edge_trees_match_the_go_file_lists` | the 24 build oracles of `oracle/sitebuild/build` (all but `build-errors`): cascade, i18n, multihost, ugly, taxonomy permalinks, aliases, collisions, custom alias template, disabled kinds/aliases/redirect, post-processing, content dirs, Thai/punctuated paths, headless and build options, `content`/`shortcodes` (with `ssg-render`'s shortcode and hook conversions), `docs` (948 pages, stub layouts) — disk builds vs the files Go wrote, and whether errors are reported | **2451/2451 files** over 25 sites; 2 accepted differences (below) |
| `docs_shortcodes::docs_cross_page_shortcodes` | docs' `include`, `glossary-term` and `quick-reference` converted to Tera on a docs-shaped site | renumbered placeholders around an include, `page_inner` of included links, a never-rendered glossary term, other sections' `.Content` and descriptions as a definition list |
| `images::referenced_images_are_processed_and_published` | a bundle image resized twice, one result printed | only the printed operation processed (through `resources/_gen/images`) and published; the bundle file published eagerly |
| `determinism::output_does_not_depend_on_threads` | A-DET: `mini`, `build-collide`, `asm-taxo`, `build-aliases` with 1, 8 and 8 threads | identical trees and collision lists |
| `smoke::smoke` (ignored) | `FUGO_SITES=<dir>[:…]` builds real site directories and prints the report | — |

Accepted differences of the edge trees (`EXPECTED` in `tests/it/edges.rs`):
`build-postprocess` `js/main.js` (the conversion, made when the offline tests had no bundler, leaves out `js.Build`) and
`css/main.css` (the page prints the asset's `.Name`, `/css/main.css`, which is also its URL:
URL-token publishing publishes it, Go publishes only on `.RelPermalink`).

Deviations from Go kept on purpose: an earlier language's `FromString` target is the one
published (Go: the later language overwrites the file); a colliding target's winner is the
later `JobOrder` (Go: the last writer); deferred output is minified with its page (Go inserts it
into the minified file).
