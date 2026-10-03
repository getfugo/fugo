# fugo (the `fugo` binary)

The command line (REWRITE_PLAN.md §2.1, §4.8, §7.5). **State: T37; T60's A-T gate and
embedded-template snapshots run through it; `server` (T71).** clap derive; `anyhow` only
here.

```
fugo [build flags]                 # no command: build (as in the Go build)
fugo build [build flags]
fugo server [build flags] [server flags]   # alias `serve`; ssg-serve
fugo templates check [project flags] [--coverage summary|full|none] [--deny-warnings]
fugo config [project flags] [--format json|toml]
fugo version                       # also --version
```

**npm packages** (feature `npm`, on by default; `crates/npm/README.md`): `build` and `server`
install the project's `package.json` into `node_modules` before they read the project, through
`ssg_build::BuildRequest::prepare` (`src/npm.rs`). They print `Installed the npm packages of
package.json in N ms` when they installed something. Nothing runs the packages' programs.
`--no-default-features --features goat,math` builds without the feature: the site installs its
`node_modules` itself.

`version` prints the Go build's line (`src/version.rs`): `fugo v<version>[-<commit>]
<os>/<arch> BuildDate=<date|unknown>[ VendorInfo=<vendor>]`, with Go's os/arch names and the
commit, date and vendor of the build-time variables `FUGO_BUILD_COMMIT`, `FUGO_BUILD_DATE`
and `FUGO_VENDOR_INFO` (set by CI's release builds).

## Flags

Kebab-case, with Go's camelCase spelling as an alias. As in the Go build (cobra), flags may
come before the command: `args::command_first` moves the command to the front before clap
parses, so `fugo -s site server` is `fugo server -s site` (a flag keeps its value:
`fugo -e server` builds with the environment `server`). The Go build's persistent flags,
`-s`, `-d`, `-e`, `--config`, `--config-dir`, `--themes-dir`, `--clock`, `-q` and `-M`,
are clap `global` flags: every command accepts them, and
one that does not use a flag ignores it (`version -s x`, `config -q -d out`), as Go did. A
boolean flag also takes pflag's explicit value, with Go's `strconv.ParseBool` spellings (`1`,
`t`, `T`, `TRUE`, `true`, `True`; `0`, `f`, `F`, `FALSE`, `false`, `False`). The flags that set
a configuration key (`-D`, `-E`, `-F`, `--ignore-cache`, `--clean-destination-dir`, `--minify`,
`--no-times`, `--no-chmod`, shown as `[=BOOL]` below) and the server's `-w`/`--watch` and
`--append-port` read it themselves, so `-D=false` overrides `buildDrafts = true` in the
configuration, as in Go; for the others (`-q=1`, `-M=false`) `command_first` turns
it into the flag (true) or drops it (false, the default).

| Flag | Alias | Commands | Effect |
|---|---|---|---|
| `-s`, `--source DIR` | | all | project directory (default: the working directory) |
| `--config A,B` | | all | configuration files, relative to the source, first wins; default: the first of `config.{toml,yaml,yml,json}` (a warning names the others when several exist) |
| `--config-dir DIR` | `--configDir` | all | `CliOverrides::config_dir` |
| `-e`, `--environment ENV` | | all | the environment (default `production`; `development` for `server`); picks `.env.<ENV>` |
| `-b`, `--base-url URL` | `--baseURL` | all | `baseURL` |
| `-t`, `--theme A,B` | | all | `theme` |
| `--themes-dir DIR` | `--themesDir` | all | `themesDir` |
| `--cache-dir DIR` | `--cacheDir` | all | `cacheDir` |
| `--ignore-cache[=BOOL]` | `--ignoreCache` | all | `ignoreCache` |
| `-D`, `--build-drafts[=BOOL]` | `--buildDrafts` | all | `buildDrafts` |
| `-E`, `--build-expired[=BOOL]` | `--buildExpired` | all | `buildExpired` |
| `-F`, `--build-future[=BOOL]` | `--buildFuture` | all | `buildFuture` |
| `--clock TIME` | | all | the build's "now" (RFC 3339 with offset) |
| `-d`, `--destination DIR` | | build, server | publish directory, relative to the source |
| `--clean-destination-dir[=BOOL]` | `--cleanDestinationDir` | build, server | `cleanDestinationDir`: the static sync removes files the static dirs lack |
| `--minify[=BOOL]` | | build, server | `minify.minifyOutput` |
| `-M`, `--render-to-memory` | `--renderToMemory` | build, server | `SinkKind::Memory`: nothing is written |
| `--threads N` | | build, server | render pool size (output does not depend on it) |
| `-q`, `--quiet` | | build, server | no summary on success; warnings and errors are still printed (the Go build's `--quiet` discarded its whole log, and had no `-q`) |
| `--no-times[=BOOL]` | `--noTimes` | build, server | `noTimes`: the static copy does not copy modification times |
| `--no-chmod[=BOOL]` | `--noChmod` | build, server | `noChmod`: the static copy does not copy permissions |

The Go build's logging and housekeeping flags (`--logLevel`, `--noBuildLock`, `--gc`,
`--printI18nWarnings`, `--printPathWarnings`, `--printUnusedTemplates`, `--templateMetrics`,
`--templateMetricsHints`) are usage errors: fugo prints warnings and errors at every level,
writes no lock file, always warns about target collisions, and has none of the others.

### `server` (alias `serve`)

Go's development server (`ssg-serve`, whose README has the details): `build`'s flags
(`-s`, `--config`, `-e`, `-b`, `-D -E -F`, `--minify`, `--clock`, `--threads`, `-q`, …; the
same `BuildArgs`), the environment `development` unless `-e` says otherwise, and:

| Flag | Alias | Effect |
|---|---|---|
| `-p`, `--port PORT` | | listen on PORT (a busy one is an error; 0: a free port); default 1313, or a free port when it is taken |
| `--bind INTERFACE` | | default `127.0.0.1` |
| `--append-port[=BOOL]` | `--appendPort` | the port goes into the base URL (default true) |
| `--disable-live-reload` | `--disableLiveReload` | no LiveReload script, `livereload.js` or WebSocket |
| `--live-reload-port PORT` | `--liveReloadPort` | the port in the LiveReload script (e.g. 443 behind a proxy) |
| `-N`, `--navigate-to-changed` | `--navigateToChanged` | the browsers go to the page whose content changed |
| `--render-to-disk` | | build into the publish directory (`-d`, `publishDir`) and serve it from there; without it the site is built into memory and `-d` is a usage error (`-M` is the default and conflicts with it). A fugo flag, not one of the Go build's: its server rendered to disk by default and `-M`/`--renderToMemory` into memory (README.md, "Upgrading from the Go build") |
| `--no-http-cache` | `--noHTTPCache` | `Cache-Control: no-store, …` and `Pragma: no-cache` |
| `-w`, `--watch[=BOOL]` | | watch and rebuild (default true; `--watch=false` builds once) |
| `--poll INTERVAL` | | poll for changes (`700ms`, `1s`, or milliseconds) instead of file notifications |

The Go server's `--disableFastRender` and `--disableBrowserError` are usage errors: every
rebuild is a full rebuild, and errors are printed, never shown in the browser.

Output (stdout; errors and warnings on stderr as `build` prints them): `Environment:
"development"`, `Serving pages from memory|disk`, `Watching for changes in <dirs>`, `Watching
for config changes in <files>`, the build summary and `Built in N ms`, `Web Server is
available at <url> (bind address <interface>)` per listener, `Press Ctrl+C to stop`; per
change `Change [of config file|of Static files] detected, rebuilding site (#N).`, the time,
`Source changed <path>` lines, then `Total in N ms`, `Synced N static file(s) in N ms`, a
build failure (`ERROR build failed: …`, the last good build is still served) or `ERROR Failed
to reload config: …`. A first build that fails exits with 1 and the build's report; a port
that cannot be opened or a configuration that does not load exits with 1.

**Environment.** Settings and the environment never come from the process environment: only
`HOME`, `XDG_CACHE_HOME`, `TMPDIR`, `USER` go to `ssg_config::load` (`ssg_build::process_env`,
`ssg_config::env::is_read`); precedence is ssg-config's (file < config dir < flags). The
project's `.env` and `.env.<environment>` files hold the variables templates read with
`get_env`.

**Exit codes** (`Exit`): 0 success; 1 build errors, check errors (or warnings with
`--deny-warnings`), or a project that does not load; 2 usage errors (clap, printed as
`error: …`). The Go build exited with 1 on every error, usage errors included, and printed
`Error: …`.

**Reports** (stderr for `build`, stdout for `templates check`):
`ERROR [id] <file>:<line>:<col>: <message>` (then `WARN `, `INFO `), notes (Tera's snippet)
indented below. Build errors that are not diagnostics print `ERROR build failed: <error>`, whose
message carries Tera's `--> <template>:<line>:<col>` snippet. A successful build prints
`pages … | files … (aliases …) | resources … | processed images … | static files …` and
`Total in N ms` (the Go build printed a statistics table per language).

### Mapping from the old port (`nh-commands`, `go-parity-final`)

| Old flag | Here |
|---|---|
| `source/s destination/d environment/e theme/t themesDir baseURL/b cacheDir ignoreCache buildDrafts/D buildFuture/F buildExpired/E clock config configDir cleanDestinationDir renderToMemory/M minify quiet` | the flags above (kebab-case + the camelCase alias) |
| `noTimes noChmod` | the flags above |
| `logLevel noBuildLock gc printPathWarnings printI18nWarnings printUnusedTemplates templateMetrics templateMetricsHints` | not supported (clap usage error, above) |
| `contentDir/c layoutDir/l disableKinds enableGitInfo panicOnWarning` | configuration keys (file or `FUGO_*`), not flags |
| `server`: `port/p bind appendPort disableLiveReload liveReloadPort navigateToChanged/N noHTTPCache watch/w poll renderToDisk` | the `server` flags above (T71; `renderToDisk` is `--render-to-disk`) |
| `server`: `disableFastRender disableBrowserError` | not supported (clap usage error) |
| `server`: `tlsCertFile tlsKeyFile tlsAuto openBrowser/O pprof renderStaticToDisk forceSyncStatic`, command `server trust` | not supported (clap usage error) |
| `devMode forceSyncStatic ignoreVendorPaths renderSegments printMemoryUsage profile-* trace`, the build's `watch/w` | not supported (clap usage error) |
| commands `new`, `mod`, `deploy`, `gen`, `list`, `convert`, `import`, `env`, `release`, `config mounts`, and cobra's `completion` and `help` | not supported (`--help` prints the help); `config` prints fugo's resolved configuration model (`ssg_config::Config`: snake_case fields, one entry per site under `sites`, the merged user keys lower-cased under `raw`) as JSON (the default; Go's was TOML) or TOML, not Go's lower-cased keys of one language (`baseurl`, `publishdir`, …), and without Go's `yaml`, `--lang` and `--printZero` |

## `templates check`

Loads the project (configuration, mounts, layouts of the project, its themes and the embedded
set) and reports every problem at once, sorted by file and line; the embedded templates are
loaded but not linted.

1. **Scan** (`ssg_layouts`): `legacy-name`, `unknown-name`, `go-template` (with the line).
   The files are left out and the check goes on.
2. **Tera.** Each template parsed alone (`tera-syntax`: Tera stops at the first syntax error),
   then all loaded as the build loads them (`ssg_layouts::load`) against the
   `spec::FUNCS` placeholders: `tera-name` (unknown filters, functions, tests, components,
   include targets, blocks), `tera-load` (missing parents, cycles). Templates with errors are
   loaded empty and the load repeated, so the rest is still checked.
3. **Context names** (`context-name`, warning): Tera's `get_template_variables` (template,
   parents, includes) outside `spec::CONTEXTS` for the role (layout job, alias, standalone,
   sitemapindex, shortcode, render hook + `HOOK_FIELDS`). Partials and components are skipped
   (kwargs, arguments).
4. **Lints** (`src/check/lint.rs`, on the tokenizer `ssg_funcs::scan` shared with the
   contract test): `kwarg` (unknown/missing kwargs), `legacy-literal` (`include`/`extends`
   literals with `_default/`, `partials/`, `shortcodes/` or upper case), `unknown-partial`,
   `call-attribute` (`.`/`?.` after a call or a parenthesised expression), `nested-close`
   (`}}` inside a `{{ … }}` expression), `content-field` (`page.content` & co. in shortcodes and
   hooks) — errors; `component-scope` (site-bound call in a component without `page=` or
   `@__nh`), `partial-component` (`partial(name="<literal>")` with arguments whose partial
   returns no value), `map-order` (map literal ranged without `sort_keys`), `none-compare`
   (`== none` / `!= none`) — warnings. A Tera syntax error at the position of a
   `call-attribute` / `nested-close` lint becomes that lint's note.
5. **Coverage** (site model loaded as a build does): the layout and base of every (page,
   format) query (`no-layout` warning when none; standalone kinds "not written"), every
   shortcode the content calls (`no-shortcode` error, with the content position), the render
   hooks of content pages (code block hooks per fence language). `--coverage summary`
   (default): templates with query counts plus every miss; `full`: every row; `none`: no model.

Output: a header line, the diagnostics, the coverage listing, `N error(s), M warning(s)`.

## Tests (`cargo test -p ssg-cli`)

| Test | What |
|---|---|
| `parity::testsite_gate_a_t` | **gate A-T** (§7.3): the testsite built by the binary against Go's `testsite-go.txtar`. L1 55/55 (Go's stats file, next to its configuration file, is not compared); L2 55/55 byte-identical, plus the §7.2 link checks (title, canonical/alternate, internal `href`/`src`/`srcset`, 15 aliases, feed `<link>`/`<loc>`/`<guid>`, JSON URL leaves, link integrity: the 10 dangling links are dangling in Go's output too); L3 visible text and heading IDs of every page; Accepted-deviation lists per level: empty. Structure oracle: the build's structure dump against `testdata/golden/testsite/structure.json` (Go's, frozen at `44529028`), every fact equal (the baseline `testdata/baselines/testsite.json` accepts none). Full output tree: `snapshots/testsite_output.snap` |
| `parity::parity_helpers` | the scanner, normalisations and text extraction of the gate |
| `docs::gate_a_d2` | **gate A-D2** (§7.3, T66): `compare.sh docs-reduced --ref golden` with this binary (Chroma, goat, emoji, math, remarshal, the recorded Tailwind CSS via `defer`, Alpine/Turbo `js_build`): L1 888/888 in both passes, L2, L4 and the structure oracle equal everywhere, A7 1.0, clean ratchet (`testdata/baselines/docs-reduced.json`); `SKIPPED` without the node modules of `tools/dev/node.sh` (`tests/it/acceptance.rs`) |
| `docs::gate_a_d3` | **gate A-D3**: `compare.sh docs-live --ref golden`, the legacy docs site without patches against the published site: L1 2372/2372, L2 and L4 equal everywhere, A7 1.0, clean ratchet (`testdata/baselines/docs-live.json`); skipped as A-D2 |
| `embedded::embedded_templates` | the embedded templates rendered against testsite views (test-only overlay `tests/it/embedded-overlay.txtar`, see below): snapshots `hooks`, `shortcodes`, `bundle`, `featured`, `section_page1`, `section_page2` |
| `embedded::embedded_templates_simple_and_disabled` | `privacy.{vimeo,x,instagram}.simple` (snapshot `shortcodes_simple`) and every service disabled |
| `embedded::embedded_template_errors` | argument errors and warnings of the embedded templates (snapshot `errors`) |
| `embedded::goat_code_block` | the goat code block hook, byte-identical to Go's output (`diagrams.Goat`'s bytes): `viewBox` of GoAT's size, `width`/`class` attributes, GoAT's SVG |
| `embedded::qr_shortcode_equals_go_s` | the `qr` shortcode against Go's `TestQRShortcode`: image names, sizes and attributes; images published |
| `build::testsite_matches_go` | `sites.py`'s testsite with `sites/testsite/layouts`, built by the binary with compare.sh's command line (`--clock … -d …`, no command) and with `build --source … --destination … --cleanDestinationDir -q`: **55/55 files byte-identical** to `crates/build/tests/it/testsite-go.txtar` |
| `build::flags_and_environment` | every configuration flag in both spellings, `-e`, no environment variable read (not the environment, not settings), `-M` writes nothing |
| `build::explicit_false_overrides_the_configuration` | `-D=false`, `--buildFuture=f` and `--cleanDestinationDir=false` against `buildDrafts`, `buildFuture` and `cleanDestinationDir = true` in the configuration: drafts and future pages left out, a stale file kept |
| `build::errors_are_reported_with_positions` | render and syntax errors with `file:line:col` and snippet, diagnostics, a missing project: exit 1 |
| `cli::version_help_and_usage_errors` | `version` and `--version` print the line of `version::BuildInfo::CURRENT`, `--help`, usage errors exit 2 |
| `cli::version_line_has_the_go_format` | the Go format with and without commit, date and vendor; Go's os/arch names for the five release targets; the version is the package's |
| `cli::removed_flags_are_usage_errors` | the Go build's logging and housekeeping flags, `--disableFastRender`, `--disableBrowserError` and the camelCase spellings no Go command line used (`--baseUrl`, `--renderToDisk`, `--noHttpCache`) exit 2; `--baseURL` and `--noHTTPCache` stay |
| `cli::explicit_boolean_values` | explicit values with Go's `ParseBool` spellings (`-DE=f`, `--watch=0`, `--appendPort=F`, …) parsed into the flags' `Option<bool>`; `--quiet=TRUE`; `config --minify` exits 2 |
| `cli::no_times_and_no_chmod_reach_the_static_copy` | `--noTimes`/`--noChmod` (both spellings): the static copy keeps or leaves the source's modification time and permissions (Unix) |
| `cli::command_first_moves_the_command_before_the_flags` | flags before the command (cobra's order): the command moved to the front, values kept (also after `=` in a short cluster), `=BOOL` of the `SetTrue` flags rewritten, everything else left for clap |
| `cli::persistent_flags_anywhere` | the Go build's persistent flags before the command and on `config`, `templates check` and `version`; a flag the command does not take is still a usage error |
| `server::server_starts_and_serves` | `serve -p 0` with camelCase flags: the start report (environment `development`, memory, watching, built), the page with the LiveReload script and `--noHTTPCache` headers, nothing on disk |
| `server::server_renders_to_disk_without_live_reload` | `--render-to-disk --disable-live-reload --watch=false -e staging`: `public/` written and served, no script, no watching |
| `server::server_start_errors` | a first build that fails exits 1 with the build's report; `-d` without `--render-to-disk`, `--render-to-disk -M`, bad `--poll`/`--port`/`--watch` exit 2; every server flag in `--help` |
| `cli::config_prints_the_resolved_configuration` | JSON / TOML, config dir, flags; a variable named like a setting is not one |
| `check::bad_layouts_report_every_rule` | `tests/it/bad-layouts.txtar`: each rule once at its position, nothing else, exit 1 |
| `check::testsite_overlay_is_clean` | the testsite overlay: 0 errors, 0 warnings; 35 (page, format) rows with `--coverage full` |
| `check::deny_warnings` | exit 1 on warnings with `--deny-warnings` |

## T60: embedded templates reviewed against Go

`tests/it/embedded.rs` renders every embedded template through the binary on the testsite plus
a test-only overlay (an `embedded` section whose layouts include the partials; pages calling
every shortcode and hook; a bundle; a `series` taxonomy; site params; x and vimeo oEmbed answers
in the getresource cache; outbound HTTP disabled through ureq's proxy variables). Go was not
built, so each snapshot was reviewed against `tpl/tplimpl/embedded/templates/**` line by line;
`rss.xml`, `sitemap.xml`, `sitemapindex.xml`, `robots.txt`, `alias.html` and the link and image
hooks are byte-identical to Go's output in A-T.

| Template | Difference found | Verdict |
|---|---|---|
| `_partials/google_analytics.html` | `gtag/js?id=G%2DABC123` (`urlencode_strict`); Go's URL escaper keeps `-` | bug fixed: `urlencode` |
| `_partials/google_analytics.html` | `gtag('config', "G-…")` (`jsonify`); Go prints `'G-…'` | bug fixed |
| `_shortcodes/details.html` | no newline before `<details`; Go's `{{- /* Render. */}}` keeps it | bug fixed |
| `_shortcodes/youtube.html` | no final newline; Go's template ends with one after `{{- end }}` | bug fixed |
| `_shortcodes/vimeo.html` | player: no `\n      ` before `<div` and no final newline; simple: a newline before `<div class="s_video_simple…">` that Go trims; simple-mode id errors used the player's wording | bug fixed |
| `_markup/render-table.html` (written natively by `ssg-markup`, whose output is this template's) | attributes in source order (Go ranges the map in key order), falsy values written, `'` unescaped and `"` as `&quot;` (Go: `transform.HTMLEscape` + `%q`: `&#39;`, `&#34;`) | bug fixed in `ssg-markup` |
| every shortcode/hook error (`instagram`, `qr`, `vimeo`, `x`, `youtube`, `param`) | `shortcode.position` / hook `position` unquoted; Go's `text.Position` prints `"file:line:col"` | bug fixed in `ssg-render` |
| all (Tera autoescape, §4.5) | `"` → `&quot;` (Go `&#34;`), `+` not escaped (Go `&#43;`), no contextual JS/URL escaping; seen in vimeo simple `alt`, hook attributes | accepted deviation (engine; equal after entity decoding, L3) |
| `_partials/pagination.html` | `format=` kwarg instead of a map with `page`; the invalid-format message names no map | accepted deviation (Tera API) |
| `_partials/opengraph.html`, `_funcs/get-page-images.html` | a string `audio`, `videos` or `images` param is a one-element list; Go's `first N` on a string gives a substring and `range` fails the build | accepted deviation (lenient where Go errors) |
| `_shortcodes/{vimeo,x}.html` simple/oEmbed failure | two warnings (`get_remote`'s and the shortcode's); Go writes one with the error appended | accepted deviation |
| `_markup/render-table.html` | the Tera file is not executed (the native writer is); it stays for lookup and precedence | accepted deviation (performance, same output) |
| `_shortcodes/highlight.html` | Chroma span structure not reviewed token by token | accepted deviation (§7.3 allowed: highlight spans) |
| `_markup/render-codeblock-goat.html` | `diagrams_goat` drew with svgbob (T66: other SVG bytes, a scoped `<style>`, whole-word `<text>`) and the template trimmed Go's blank lines; now a port of GoAT v0.5.0 (feature `goat`, on by default) and Go's whitespace: the hook's output equals Go's byte for byte (`goat_code_block`; the docs' GoAT pages) | bug fixed |
| `_shortcodes/qr.html` | argument checks equal Go's messages; images named and sized as Go's (`qr_shortcode_equals_go_s`), their bytes equal Go's (`ssg-images` QR tests) | none |
| figure, instagram (body byte-equal to Go's `render-instagram`), x, param, ref, relref, opengraph, twitter_cards, schema, `_funcs/get-page-images`, render-link, render-image | none | – |

`.Summary` and `.WordCount` values the partials print are the engine's and are not reviewed here.
