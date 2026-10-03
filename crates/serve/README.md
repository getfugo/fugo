# ssg-serve

`fugo server` (REWRITE_PLAN.md T71): the Go implementation's development server. axum 0.8
(HTTP/1 and the LiveReload WebSocket on one port) on a current-thread tokio runtime, notify 8
with notify-debouncer-full 0.7 (1 s debounce), full rebuilds through `ssg_build::build`.
**State: T71.**

```rust
pub struct ServeOptions { pub build: BuildRequest /* project + build flags */, pub bind: String,
                          pub port: Port, pub append_port: bool, pub live_reload: Option<LiveReloadOptions>,
                          pub target: Target, pub watch: Watch, pub http_cache: HttpCache }
pub enum Port { Exact(u16) /* --port; 0: any free port */, Preferred(u16) /* 1313, else a free one */ }
pub enum Target { Memory /* default */, Disk /* --render-to-disk, fugo's flag */ }
pub enum Watch { Off /* --watch=false */, Native, Poll(Duration) /* --poll */ }
pub enum HttpCache { Default, Disabled /* --noHTTPCache */ }
pub struct LiveReloadOptions { pub port: Option<u16> /* --liveReloadPort */, pub navigate_to_changed: bool }
pub trait Reporter: Send + Sync { fn report(&self, event: &Event<'_>); }   // the CLI prints the events
pub struct Server;  // start(&ServeOptions, &Arc<dyn Reporter>) -> Result<Server, ServeError>,
                    // urls(), local_addrs(), wait(), shutdown()
```

## Start (`Server::start`)

1. The configuration is loaded as `build` loads it (the CLI defaults the environment to
   `development`).
2. **Listeners** (Go's `createServerPorts`): one, or one per language of a multihost site
   (`--appendPort=false` is refused there), on `--bind` (127.0.0.1): the port, then each next
   one; a busy port is replaced by a free one unless `--port` asked for it (the first).
3. **Base URLs** (Go's `fixURL`): every language's base URL becomes `--baseURL`, else the
   configured one on `localhost` over `http`; with `--appendPort` (default) the listener's port
   replaces the URL's. A base URL without a host (`/`) becomes `//localhost:<port>/`, as in Go.
   The rewritten configuration goes to the build (`BuildRequest::config`), and the build's
   `LiveReload` puts `<script src="<base path>/livereload.js?mindelay=10&v=2&port=<port>&path=<base
   path>/livereload" data-no-instant defer>` at the start of the head of every HTML page (not
   alias redirects; `ssg-publish`); `--liveReloadPort` changes the port in it.
4. The watcher starts (before the first build, so nothing changed during it is lost), then the
   first build; if it fails the server does not start (as in Go).
5. The HTTP thread serves; the watch thread handles changes.

## HTTP (`src/http.rs`, `src/tree.rs`)

| Request | Response |
|---|---|
| `<base path>livereload.js` | the Go implementation's `livereload.min.js`, `text/javascript` |
| `<base path>livereload` | the LiveReload WebSocket (Go's origin check: no `Origin`, the same host, or the same host name on another port; else 403) |
| a file of the last good build | the bytes; `Content-Type` from the site's media types (`text/*` with `charset=utf-8`, the later type wins a suffix, as the Go implementation registers them with Go's `mime`), else `mime_guess`, else sniffed text/binary; `Accept-Ranges: bytes` and one byte range (206/416) |
| `…/` | `…/index.html` |
| `…/dir` with `…/dir/index.html`, `…/index.html`, a file with a trailing `/` | Go's `http.FileServer` redirects (301 `dir/`, `./`, `../name`; query kept) |
| a miss that is a navigation (`Sec-Fetch-Mode: navigate`, or a path ending in `/`, `html`, `htm`, or without `.`) | status 404 with the `404.html` of the language directory the path is in (`/nn/…` → `/nn/404.html`), else the site's `/404.html`, else the first language's, else `<h1>Page Not Found</h1>` (Go's default `[[server.redirects]]` `/** → /404.html`, made per language) |
| any other miss | Go's `404 page not found` (`text/plain`) |
| outside the base URL's path | `/docs` → 301 `/docs/`; else the plain 404 |

A multihost site's listener serves its language's directory (`en/`), with that language's
404 page. `--noHTTPCache` adds `Cache-Control: no-store, no-cache, must-revalidate,
max-age=0` and `Pragma: no-cache`. Files come from the memory sink of the last successful
build (swapped in whole: a failing build leaves it in place), or with `--render-to-disk` from
the publish directory (written in place by each build, so a failing build can leave some
files new). `--render-to-disk` is fugo's flag, not one of the Go build's: the Go server
rendered to disk by default and into memory with `-M`/`--renderToMemory`.

## Watching and rebuilding (`src/watch.rs`, `src/rebuild.rs`)

**Watched** (Go's `WatchFilenames` + config files): every mount of the project and its
themes (content, layouts, assets, data, i18n, archetypes, static; `disableWatch` mounts left
out) recursively, a mounted file through its directory, the configuration directories (the
project's `--configDir` and each theme's `config/`) recursively, and, not recursively, the
directories of the configuration files `Config::config_files` lists (the project's and its
themes') and the project's and each theme's directory. **Configuration** is any of those
files, anything below a configuration directory, and a `config.*` file in the project's or a
theme's directory (so a new `config.toml` next to a `config.yaml` is a configuration change,
and wins). The project's `.env` files and its `package.json` count as configuration files
whether or not they exist yet. Each time the server loads the configuration, it runs the
request's `BuildRequest::prepare` before it builds the `Vfs`: at the start, and on every
configuration change. The CLI installs the project's npm packages there, so saving
`package.json` installs again.
notify's native watcher (inotify, FSEvents, …) or its poller (`--poll 700ms`; a number is
milliseconds; the poller compares contents, because notify keeps modification times in
whole seconds), debounced by notify-debouncer-full: an event is delivered 1 s after it
happened (100 ms ticks); batches that arrive during a build are handled together. After each
batch the watch set is computed again, so a component directory created while serving (a
first `static/` or `assets/`, seen through the project directory's watch) is watched from
then on, and one removed and created again is watched anew (Go misses both).
Before the debouncer sees them, the native watcher's events are put in the watched paths'
terms (`NativeWatcher`): macOS's FSEvents reports a watched directory's resolved path
(`/private/var/…` for `/var/…`, a symlink's target), which no mount starts with; and FSEvents
repeats a file's earlier flags with a later event, so a removal arrives as "created, removed,
modified", which the debouncer would drop as a file that came and went. On macOS an event
(but a rename) on a path that no longer exists is therefore a removal.

**Ignored**: editors' temporary and backup files (Go's list: `~`, `.swp`, `.swx`, `.bck`,
`.tmp`, `4913`, `.goutputstream*`, JetBrains `___jb_*___`, `.sb-*`, `#…`, `.#…`), names
starting with `.`, anything below `.git`, `node_modules` or `bower_components` inside a
mount, permission and time changes,
opening, reading and closing (a write is seen as a modification), and files created or
written that are gone again.

| A batch with | Does | Then sends |
|---|---|---|
| configuration (above) | reloads the configuration (and the watch set), rebuilds everything; a configuration that does not load pauses everything else until it loads (as in Go); a site that becomes or stops being multihost needs a restart | a full reload |
| content, layouts, assets, data, i18n, archetypes (or lost events) | a full rebuild into a new memory sink, served when it succeeds; a failure is reported with positions (as `build` reports) and the last good build stays | Go's fast-render rules on the files that changed against the last good build (source maps left out): none, nothing; content changed, a full reload, or with `--navigateToChanged` the bundled plugin's navigate prefix (`NAVIGATE_PREFIX` in `src/livereload.rs`) followed by the page path, with the page's server port in `overrideURL`; one other file, that path; stylesheets only, each stylesheet (applied in place by livereload.js); else a full reload, then the stylesheets after 200 ms. `--render-to-disk` builds are not compared: a full reload |
| static files only | no build: the static mounts are listed again, each file below a changed path is copied into the served tree (memory, or the publish directory), a file no static directory has any more is removed. Files whose bytes did not change are left alone. As in Go, a static file wins over a rendered file of the same path until the next build | one changed file: its path (a stylesheet or image is updated in place); several: a full reload; none: nothing |

A full reload is Go's `{"command":"reload","path":"/x.js","originalPath":"","liveCSS":true,
"liveImg":true}`; the hello answer is `{"command":"hello","protocols":
["http://livereload.com/protocols/official-7"],"serverName":"fugo"}`.

`Server::shutdown` stops the watch thread (after a build in progress) and the HTTP server
(graceful: open requests finish, WebSockets get a close frame). The CLI has no signal
handling (tokio's `signal` feature is not locked): Ctrl+C ends the process at once, which is
harmless for memory builds.

## Deviations from the Go implementation

- Every change is a full rebuild (as with Go's `--disableFastRender`, without the partial
  rebuilds):
  no fast render mode, no `RecentlyTouched` page list; the fast-render reload rules are applied
  to what the rebuild changed.
- No error page in the browser (as with Go's `--disableBrowserError`): a failing
  build is printed and the browser keeps the last good page (no reload is sent).
- `[server]` `headers` and `redirects` are not read; the default 404 redirect is built in, per
  language.
- The site's `404.html` is chosen per language directory (Go: `/404.html` unless configured).
- No TLS (`--tlsCertFile`, `--tlsKeyFile`, `--tlsAuto`, `trust`), `--openBrowser`, `--pprof`,
  `--renderStaticToDisk`, `--forceSyncStatic`, `--disableFastRender` and
  `--disableBrowserError` (clap usage errors).
- A busy default port falls back to a free port on `--bind` (Go listens on all interfaces
  then). Directory listings are not served (Go's `filesOnlyFs` lists nothing either; a
  directory without `index.html` is a miss).
- The WebSocket answers `serverName` `fugo`. One byte range per request (Go serves
  multipart ranges); no `Last-Modified`/`ETag`.
- The polling watcher reads the watched files at every interval (see above).

## Tests (`cargo test -p ssg-serve -- --nocapture`)

| Test | What |
|---|---|
| unit: `address::base_urls` | `fixURL` cases (localhost, https → http, base paths, host-less and scheme-less URLs, `--baseURL`, `--appendPort=false`, IPv6) |
| unit: `livereload::{messages,origins}` | reload/navigate JSON, Go's origin check |
| unit: `http::{ranges,paths}` | byte ranges, path cleaning and decoding, navigation detection |
| unit: `watch::ignored_names` | editors' temporary files |
| unit: `watch::events_are_mapped_to_the_watched_paths`, `watch::events_on_gone_paths_are_removals_on_macos` | resolved paths mapped back (the most specific watched path), creations and writes of gone paths as removals on macOS |
| `serve::serves_the_site` | pages with the script after `<head>`, directory and `index.html` redirects, an alias without the script, static and asset files and their types, the 404 page for navigations and the plain 404 otherwise, `livereload.js`, the WebSocket handshake, a foreign `Origin` refused, `HEAD`, byte ranges |
| `serve::content_change_rebuilds_and_reloads` | edit → full reload → the new page; the removed alias is gone; 2 builds |
| `serve::static_change_copies_without_a_rebuild` | a static edit, a removal and a new file: the path reloaded, the file served or gone, still 1 build |
| `serve::reloads_follow_what_the_build_changed` | an asset stylesheet → its path; a layout changing one file → that path; several → a full reload; the same bytes → a build and no reload |
| `serve::a_broken_build_keeps_the_last_good_site` | a template syntax error is reported with the file, the old page is served, no reload; the fix reloads |
| `serve::config_change_reloads_the_configuration` | title change → full reload; broken configuration → paused (content edits wait); fixed → rebuilt with both changes |
| `serve::navigate_to_the_changed_page` | the navigate prefix and `/posts/one/`, with the port |
| `serve::render_to_disk` | the publish directory written and served; static copied there; rebuild |
| `serve::base_path_caching_and_no_live_reload` | `/docs/` base path, `/docs` redirect, outside paths, `--noHTTPCache`, no script or endpoints without live reload, `--watch=false` |
| `serve::multihost_sites_get_a_listener_each` | two listeners, each language's base URL with its port, its script and 404 page, a WebSocket each |
| `serve::per_language_404_pages` | `/nn/…` → `nn/404.html`, other misses → `404.html` |
| `serve::a_new_static_directory_is_watched` | a site without `static/`: the new directory's file is copied, and a second edit inside it is seen (no build) |
| `serve::theme_and_new_config_files_are_watched` | a theme's layout (→ its one changed page) and its `config.toml` (→ reloaded configuration), then an edit of the project's `config.toml` (→ reloaded configuration) |
| `serve::polling_watcher` | `--poll 100ms` picks up an edit |
| `serve::build_is_server_and_site_server_port` | `build.is_server` true and `site.server_port` the listener's port in the server (`BuildRequest::server`, T70); `false` and 0 in a `build` of the same site |
| `testsite::testsite_is_served_and_reloads` | the testsite: pages of both languages and formats with the canonified server URLs, no script in JSON and aliases, static, types, both 404 pages, then edit → reload **measured**: 3 content edits, 1 layout edit, 1 static edit (no build) |

Measured on the testsite (debug build; 4 CPUs shared with other builds):

| Run | content edit → reload | layout edit | static edit (no build) |
|---|---|---|---|
| `fugo server`, alone (3 sessions, 13 content and 9 static edits) | 1.54–1.84 s, one 2.17 s while the machine was compiling | – | 1.00–1.14 s |
| `testsite_is_served_and_reloads` (8 runs, 6 of them next to the other tests) | 1.54–2.03 s (23 of 24 ≤ 2 s) | 1.55–1.69 s | 1.01–1.07 s |
| the same test after the highlighter cache (below) | 1.09–1.11 s | 1.11 s | 1.06 s |

That is the 1 s debounce, up to one 100 ms tick, and the rebuild. Before the cache, callgrind
on a testsite build found **92 % of the rebuild (≈0.52–0.76 s in the debug build) in
`ssg_highlight::Highlight::new`** (`Languages::load` → syntect `SyntaxSetBuilder::build`,
which dumps, deflates and reloads the syntax set), on every build, although the testsite
highlights nothing. `ssg-highlight` now loads its lexers and styles once per process and
every `Highlight` shares them, so a rebuild after the first costs tens of ms (since the Chroma
port replaced syntect, `Highlight::new` reads the lexer configurations, ≈7 ms in a dev build,
and a lexer compiles its rules on first use).
