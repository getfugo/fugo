# fugo Rust rewrite: handoff (start here)

State as of T70 (2026-09-30) and the removal of the Go implementation (2026-10-01, §9), branch
`rust-port`. The plan and its task table are [`REWRITE_PLAN.md`](REWRITE_PLAN.md) (§8.2 holds
every task's state); the workspace's own rules and CI details are
[`DEVELOPMENT.md`](../../DEVELOPMENT.md). This document says what exists, how
to build, test and run it, how the gates work, what deviates from the Go implementation, and
what is open.

## 0. Summary for the pull request

fugo is an idiomatic Rust rewrite of the Go version (itself a fork of an older Go static site
generator) and replaces it (§9; renamed fugo on 2026-10-02, §10). The Cargo workspace
is at the repository root: the Go implementation's site and page model (content tree, bundles,
kinds, front matter, cascade, permalinks, output formats, taxonomies, menus, pagination, i18n,
asset pipes, image processing, Markdown with render hooks and shortcodes) with **Tera 2**
templates instead of Go templates (decision D4). It is not a byte-for-byte port: the Go build was the oracle (its
outputs are frozen as golden data, §9), and outputs are compared structurally.

- **Gates passed** (REWRITE_PLAN.md §7.3; each against the Go version's build of the same site):
  - A-T, the testsite: L1 55/55, L2 and L3 equal on every page, structure oracle equal.
  - A-D1, the legacy docs site with the `i01` patches: L1 887/887, L2 756/756, L3 749/749, A7 1.0.
  - A-D2, the legacy docs site with the `reduced` patches (Chroma-class highlighting, GoAT diagrams,
    emoji, math, the recorded Tailwind CSS, the real Alpine/Turbo `js_build`): L1 888/888, L2
    757/757, L3 750/750, A7 1.0.
  - A-D3, the legacy docs site (`testdata/legacy-docs`) **without patches against the published
    site** (getfugo.github.io at a1928152, the Go build of 2025-10-13; T74): L1 2372/2372, L2
    776/776, L3 769/769, L4 873/873, A7 1.0 — every page's visible text equals the published
    one. `tools/legacy-docs/build.sh` builds the site.
  - The Go build's stats file (next to its configuration) is not compared: fugo writes none
    (T75).
  - A-DET: identical output with 1 and 8 threads and across runs.
- **A-P, performance** (T70; release build, 4 CPUs, medians of 5–6 runs): the docs site builds
  cold in 3.29 s against Go's 3.99 s (0.82×; goal ≤ 1.5×), warm in 3.26 s against 3.51 s
  (0.93×; goal ≤ 1.0×), peak RSS 384 MB (goal ≤ 1 GB). The testsite is faster than Go as
  well (§5).
- **Commands:** `fugo build` (also with no command), `server` (live reload, memory or
  disk), `templates check`, `config`, `version`.
- **CI/CD:** `.github/workflows/ci.yml`, the only build workflow (fmt, clippy `-D warnings`,
  licence check, the whole test suite with the gate tests, release builds for five targets);
  tags `v<version>` publish a GitHub release with archives, licence notices and checksums.
- **Go removed** (2026-10-01): the Go implementation, its tools and its workflows are gone; what
  it generated is frozen at commit `44529028` (§9).
- **Known deviations** from the Go implementation are listed and classified (§7); none is silent.
- **Open:** the migrate tool (T73), the
  remaining COULD features (T72: `:git` lastmod, Org front matter), server extras (§8).

## 1. What exists

```
Cargo.toml, Cargo.lock      the Cargo workspace at the repository root (26 crates), with
                            DEVELOPMENT.md, PROVENANCE.md, THIRD_PARTY/, deny.toml
crates/<name>/              one crate each; README.md per crate (API, state, accepted deviations)
sites/<site>/               the Tera layouts of the test sites (testsite, docs + patches)
testdata/                   Go-oracle fixtures (oracle/), corpora, golden Go-build data (golden/),
                            the Go tree's test data (upstream/), the ratchet baselines (baselines/)
tools/dev/              harness: compare.sh, structdiff.py, manifest.py, selftest.py,
                            node.sh, licence-check.sh, notices.py, package.py,
                            changes/ (the ratchet's changes files)
tools/rust-port/i01/        sites.py (generates every test site), patches.json, site txtars
.github/workflows/ci.yml    CI and releases of fugo
docs/rust-port/             this file, template-api.md (the template API, generated from
                            crates/funcs/src/spec.rs), REWRITE_PLAN.md, archive/ (the old
                            port's docs)
```

The old byte-for-byte port (the root `crates/` of `be02933a`, not today's crates; line-by-line
ports of Go packages) was deleted in T00; it is at that commit (local tag `go-parity-final`).
Its documents are in [`archive/`](archive/README.md). The Go implementation (the Go tree,
`tools/go-oracle`, `tools/dev/oracle.sh`) is at commit `44529028` (§9).

### Crate map

Dependencies point down the table (lower crates never depend on higher ones). Lines are
`src` + `tests` at T70.

| Crate | Package | Role | Lines |
|---|---|---|---|
| `base` | `ssg-base` | shared vocabulary: `Value`/`Map`/`Params`, dates, paths and URLs, `IdVec`, diagnostics, inflection and title case, globs | 4.9k + 1.5k |
| `config` | `ssg-config` | configuration pipeline (normalise, legacy keys, merge, per language, themes, `FUGO_*`) and the typed `Config` | 6.5k + 3.1k |
| `vfs` | `ssg-vfs` | mounts → one union view per component, walkers, the path parser (file → language, format, bundle kind, key) | 1.7k + 1.5k |
| `pageparser` | `ssg-pageparser` | content files: front matter, summary divider, shortcode lexing and assembly | 1.7k + 0.8k |
| `locale` | `ssg-locale` | ICU4X collation, plurals, numbers and dates; i18n bundles with the `{{ .Field }}` evaluator | 1.7k + 1.3k |
| `page` | `ssg-page` | per-page rules: dates, permalinks, cascade matching, build options, menus in front matter | 2.5k + 1.8k |
| `site` | `ssg-site` | capture and assembly into the `Model`: page tree, kinds, sections, taxonomies, translations, resources, data | 3.7k + 2.8k |
| `nav` | `ssg-nav` | menus, pagination and pager URLs, related content, the alias plan | 1.6k + 2.2k |
| `markup` | `ssg-markup` | Markdown through comrak behind an engine-neutral API, plus the Go implementation's passes (goldmark's pipe tables, heading IDs, attributes, deflists, alerts, passthrough, emoji, linkify, typographer, TOC, context markers) | 4.5k + 3.7k |
| `highlight` | `ssg-highlight` | code highlighting: a port of Chroma v2.19.0 (its XML lexers converted to Rust data, its Go lexers ported, its regex-lexer engine on a port of the regexp2 dialect, its HTML formatter), Chroma class names or inline styles from Chroma's style files | 2.6k + 0.8k |
| `minify` | `ssg-minify` | output minification (minify-html, lightningcss, oxc) | 1.5k + 1.3k |
| `images` | `ssg-images` | image processing (resize, fit, fill, crop with Go's smart crop, filters, text, QR, dither, EXIF), the image cache | 6.3k + 3.3k |
| `jsbuild` | `ssg-jsbuild` | `js_build` in process: rolldown with fugo's plugin (assets-first resolution, `@params`, `inject`, CSS imports), TC39 decorators and the `es5` target | 9.6k + 5.8k |
| `resources` | `ssg-resources` | the `ResourceStore`: assets, page resources, pipes, all in process (Sass via grass, `js_build`, minify, fingerprint, `execute_as_template`, `post_process`), `get_remote` with its cache | 5.0k + 3.2k |
| `publish` | `ssg-publish` | sinks, canonify/absolute URLs, URL-token extraction, held outputs, the HTML elements `purge_css` keeps, static sync | 1.9k + 1.3k |
| `layouts` | `ssg-layouts` | layout scan and lookup (the Go implementation's v0.146 names and scoring), embedded templates in Tera | 2.4k + 1.7k |
| `funcs` | `ssg-funcs` | the template API (`spec.rs`, the single source of truth) and the pure functions (with `to_math`: KaTeX in QuickJS; `diagrams_goat`: the bep/goat port) | 4.9k + 1.2k |
| `view` | `ssg-view` | the serialisable views templates read (`page`, `site`, `build`, …) and their caches | 2.8k + 1.3k |
| `sitefuncs` | `ssg-sitefuncs` | site-bound Tera functions (`get_page`, `ref`, `i18n`, resources, images, `paginate`, `partial`, `defer`, …) | 3.0k + 1.9k |
| `render` | `ssg-render` | the render `Session`: content (shortcodes, hooks), layout jobs, waves | 2.7k + 1.3k |
| `build` | `ssg-build` | build orchestration (phases B–E7 of REWRITE_PLAN.md §3; content adapters before the model), `BuildRequest`/`BuildReport` | 1.1k + 2.0k |
| `serve` | `ssg-serve` | `fugo server`: listeners, file serving, LiveReload, watching, rebuilds | 2.4k + 1.0k |
| `npm` | `ssg-npm` | the project's npm packages: `package.json` installed into `node_modules` before a build (Deno's installer, `npm.lock`); nothing runs their programs | 0.4k + 0.2k |
| `cli` | `fugo` | the `fugo` binary (clap); the gate tests live in its `tests/it` | 1.9k + 2.0k |
| `migrate` | `ssg-migrate` | stub (T73: Go-template → Tera converter) | – |
| `testkit` | `ssg-testkit` | dev-only: fixture readers, txtar sites, the template contract test, a local npm registry | 0.9k + 0.7k |
| `workspace-hack` | `ssg-workspace-hack` | feature unification of shared dependencies | – |

## 2. Build, test, run

Environment (every checkout and worktree shares one target directory):

```sh
export CARGO_TARGET_DIR=<main checkout>/target CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
cargo build --release --offline --locked -p ssg-cli      # → $CARGO_TARGET_DIR/release/fugo
cargo test -p ssg-<crate> --offline --locked           # the edit–test loop (cli: -p ssg-cli)
```

The full check CI runs (workspace-wide; not for the edit–test loop):

```sh
tools/dev/node.sh check                               # once: tools/dev/node.sh
cargo test --workspace --offline --locked             # includes the gate tests A-T, A-D2, A-D3
cargo fmt --all --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
tools/dev/licence-check.sh
python3 tools/dev/selftest.py                         # the harness's self-test
python3 tools/rust-port/i01/sites.py patches --check      # patches.json ↔ sites/docs/patches
```

A test whose external tool is missing prints `SKIPPED …` and passes (DEVELOPMENT.md lists the
tools); run with `-- --show-output` and grep for `SKIPPED` before trusting a green run. After
switching worktrees, `touch` the sources of the crates you test (shared target directory; see
DEVELOPMENT.md).

Running it (the Go program's flags in kebab-case, the camelCase spellings as aliases; the full table is
`crates/cli/README.md`):

```sh
fugo [build] -s <site> [-d <out>] [--minify] [-b <url>] [-e <env>] [-D -E -F] [--clock <rfc3339>]
fugo server -s <site> [-p 1313] [--bind 127.0.0.1] [--render-to-disk] [--disable-live-reload] [--poll 1s]
fugo templates check -s <site> [--coverage summary|full|none] [--deny-warnings]
fugo config -s <site> [--format json|toml]
fugo version
```

`FUGO_TIMINGS=1` prints the build's phase timings on stderr (T70).

**Configuration.** The project file is the first of `config.{toml,yaml,yml,json}` (a warning
names the others when several exist; the Go program's configuration file name is not read),
plus `config/` dirs, merged with themes as the Go implementation does
(`crates/config/README.md`).

**fugo's own names** (2026-10-01). Every name that was the Go program's is fugo's, with no
fallback: the configuration file `config.*` (not the Go program's name), the
default cache directory `fugo_cache`, the generator meta `fugo <version>`, `@import "build:vars"` in Sass,
`package.config.json` among the JS config files, the reserved layouts directory `_internal/`
and the ids of `js_build`'s virtual modules (`ssg:entry`, `\0ssg-params`, …). The
tests replay the Go oracles with these names (`ssg_testkit::fixture::local_path`, the
harness's `sites.py as_local_site`). Templates read the build's version and environment as
`build` (`build.environment`, `build.is_production`, `build.is_development`,
`build.is_server`, `build.version`, `build.generator`; `@build` in components). Every
environment variable fugo reads or sets is `FUGO_*`: the harness switches
(`FUGO_TIMINGS`, `FUGO_STRUCTURE_OUT`, …), the
default `get_env` allowlist (`^FUGO_`). Settings and the environment never come from the process environment
(2026-10-03: the `FUGO_TITLE`, `FUGO_PARAMS_X`, `FUGO_CACHEDIR` overrides and
`FUGO_ENVIRONMENT` were removed; `--environment` chooses the environment, the harness passes
`--cacheDir`). **The asset pipeline is Rust only** (2026-10-03, T75): no PostCSS, Babel or
Tailwind pipe, no stats file (`[build] buildStats`/`writeStats` are "no longer supported"
warnings), and fugo runs no programs (`[security.exec]` is accepted and has no effect). `minify`
adds vendor prefixes for the browserslist, `js_build` lowers for the browser targets,
`purge_css` purges per page, and Tailwind sites run Tailwind's own CLI next to fugo
(`docs/content/asset-pipelines/tailwind-css.md`). Secrets go in the project's `.env` file (`ssg_config::env_file`):
templates read its names with `get_env` (no allowlist entry; the process environment wins), it
is never configuration (`fugo config` does not print it), `FUGO_*` names in it are ignored with a
warning, and `fugo server` reloads when it changes. Layouts must be Tera with the Go implementation's v0.146 names (`home.html`, `single.html`,
`_partials/`, `_shortcodes/`, `_markup/`); legacy names are errors with a hint. The template
API (every function, filter, test and context, with the Go implementation's name for each) is
`docs/rust-port/template-api.md`; `fugo templates check` checks a site's templates against it.

**npm packages** (2026-10-03, `crates/npm/README.md`). fugo needs neither Node.js nor npm.
`build` and `server` install the dependencies of the project's `package.json` into
`node_modules` (Deno's installer as a library: `.npmrc`, integrity checks, npm's hoisted layout,
the platform's optional packages, no install scripts). They lock the versions in `npm.lock`
(Deno's format, seeded from `package-lock.json`) and download into `<cacheDir>/packages`. The
server installs again when `package.json` changes. A `node_modules` that npm, pnpm or yarn
wrote, or a link, is left alone. The harness links `tools/dev/node_modules` into its sites, so
gate builds install nothing; the tests find it with `tools/dev/node.sh path`
(`ssg_testkit::fixture::node_tools`). Nothing runs the packages' programs: the JavaScript
runtime that ran Tailwind (Deno's `deno_runtime`, V8) was removed with the Tailwind pipe (T75).
The default feature `npm` of `ssg-cli` carries the installer: about 6 MB of the stripped release
binary, 71.0 MB instead of 64.8 MB (macOS arm64; DEVELOPMENT.md "Optional features").

## 3. Gates and the harness

The oracle is the golden data the Go build wrote to `testdata/golden/<label>/` (manifests
of a minified and an unminified build, the structure dump; `tools/dev/oracle.sh` with the Go
binaries), frozen at `44529028` (§9; `testdata/golden/README.md` has the recipe to
regenerate it in a worktree of that commit). Sites are generated outside the repository by
`tools/rust-port/i01/sites.py make <site> <dir> [--overlay sites/<site>] [--docs-patches
i01|reduced]`; the Rust side replaces the layouts with the overlay's Tera files.

`tools/dev/compare.sh <label> [--ref golden] [--task Txx] [--update] [--report-only]`
builds the Rust side (both passes, plus its structure dump), compares with `structdiff.py` at
the levels of REWRITE_PLAN.md §7.2 (L1 file set, L2 links and URLs, L3 visible text and
heading IDs, L4 assets, S the structure oracle, A7 the share of pages with equal text) and
checks the result against the **ratchet**: the baseline `testdata/baselines/<label>.json`
may change only through entries in `tools/dev/changes/<task>.md`, each with one class
(`engine-difference`, `bug-fixed`, `accepted-deviation`) and a reason; an unlisted difference
fails (`tools/dev/changes/README.md`).

| Gate | Site (label) | How it runs | State |
|---|---|---|---|
| A-T | testsite | `cargo test -p ssg-cli --test it parity` (`testsite_gate_a_t`; in-process comparison with the Go build's output `crates/build/tests/it/testsite-go.txtar` and the structure oracle); also `compare.sh testsite` | passed (T60, T03) |
| A-D1 | docs-i01 | `tools/dev/compare.sh docs-i01` (not a committed test) | passed (T65) |
| A-D2 | docs-reduced | `cargo test -p ssg-cli --test it docs` (`gate_a_d2`: `compare.sh docs-reduced --ref golden`) | passed (T66; every page equal since T74) |
| A-D3 | docs-live | `cargo test -p ssg-cli --test it docs` (`gate_a_d3`: `compare.sh docs-live --ref golden`; the golden data is the published site, testdata/golden/README.md) | passed (T74) |
| A-DET | mini, edge trees | `cargo test -p ssg-build --test it determinism` | passed (T36) |
| A-P | docs (release) | §5 | goals met (T70) |

The gate tests need python3, bash, node and the node tools (else `SKIPPED`).

## 4. CI/CD

`.github/workflows/ci.yml` is the repository's only build workflow (`bump.yml` cuts releases,
`stale.yml` manages issues).
It runs on pushes to `main` and `rust-port`, on every pull request (no path filters), on
`v[0-9]*` tags and by hand. Jobs: **Lint** (fmt, clippy `-D warnings`, licence check, structdiff
self-test, `sites.py patches --check`, a tag is a release tag), **Test** (Linux only, §8: the
whole workspace with the node modules of `tools/dev/node.sh`; a test that
prints `SKIPPED` fails the job), **Build** (release for `x86_64`/`aarch64` Linux,
`x86_64`/`aarch64` macOS, `x86_64` Windows, with the version (a tag's), commit, date and
vendor of `fugo version`; `notices.py` writes `THIRD_PARTY_NOTICES.txt`, `package.py` the archive), **Release**
(tags only: the GitHub release `v<version>` with the five archives and
`fugo_<version>_checksums.txt`; a version with a `-` makes a pre-release, any other is latest
only if no release has a higher version). Cutting a release: run **Bump version** (`bump.yml`,
`version_bump` major/minor/patch), which tags the branch head `v<next>` and starts CI on the
tag; nothing in the repository is edited (DEVELOPMENT.md "CI and releases").

## 5. Performance (A-P, T70)

Release build of the Rust `fugo` (default features), the Go `fugo` that `oracle.sh install`
built for T01 (`go build -trimpath -ldflags="-s -w"`; the Go tree and `oracle.sh` are at
`44529028`, §9).
Each run: a freshly generated site (`sites.py make`; Rust with its overlay), the environment of
`compare.sh` (clean env, `TZ=UTC`, HTTP disabled, the golden GetRemote cache, node modules on
`PATH`) but with each implementation's default parallelism (no worker-multiplier variable),
`--clock 2026-09-27T12:00:00Z --minify -d <out>`. **Cold:** no `resources/`, empty cache
directory. **Warm:** the same site again, `resources/_gen` (image cache) and cache directory
kept, output removed. Wall time and the peak RSS of the fugo process (`wait4`; external
tools such as Tailwind and, at the time, esbuild not included: `js_build` has run in process
since 2026-10-02 and was not measured again). Machine: 4 CPUs, 15 GB RAM, nothing else
running.

| Site | | Go median (min–max) | Rust median (min–max) | Rust / Go | Peak RSS Go / Rust |
|---|---|---|---|---|---|
| docs-reduced (888 files) | cold | 3.99 s (3.86–4.13) | 3.29 s (3.11–4.48) | **0.82** (goal ≤ 1.5) | 399 / 378 MB (max 384) |
| | warm | 3.51 s (3.47–4.81) | 3.26 s (3.09–4.51) | **0.93** (goal ≤ 1.0) | 290 / 359 MB (max 365) |
| testsite (55 files) | cold | 0.099 s (0.085–0.124) | 0.071 s (0.064–0.080) | 0.71 | 71 / 44 MB |
| | warm | 0.090 s (0.083–0.101) | 0.068 s (0.064–0.077) | 0.75 | 71 / 43 MB |

Go n=5, Rust n=5 (docs n=6); the single slow outliers (≈ 4.5 s) of both sides were other
activity on the shared machine. Before T70's fix the Rust docs build took 3.83 s cold and
3.63 s warm (one run each), the testsite 0.50 s: every process linked the syntect syntax set
(≈ 0.45 s); `build.rs` now links it at compile time.

Where the Rust docs build spends its time (`FUGO_TIMINGS=1`, warm, minified): model 40 ms,
templates 40 ms, content 630–800 ms (Markdown and highlighting of ~3,900 fences), wave 1
750–900 ms (888 layouts), deferred 1.4–1.6 s (Tailwind ≈ 0.55 s, then placeholder patching,
**HTML minification** and writing of every held page), resources 20–190 ms (images). These
numbers predate T75, which removed the Tailwind pipe: the docs builds now publish the
stylesheet Tailwind built, so the deferred phase no longer pays Tailwind's 0.55 s. Without
`--minify` the deferred phase is ≈ 0.7 s shorter; Go's minifier costs it ≈ 0.25 s. Proposals,
not done:
1. HTML minification: `ssg-minify` runs minify-html twice on pages with comments or
   omitted end tags (for idempotence); fold the second pass into one (strip comments before,
   or check whether a second pass can change anything) and minify pages while wave 1 renders
   them when they hold no deferred placeholder.
2. Warm builds gain little because image processing is already cheap (Go saves ≈ 0.5 s warm,
   Rust ≈ 0.15 s); the remaining cost is rendering, so wave-1 profiling (Tera value cloning of
   page views) is the next step if the warm ratio needs to drop further.

## 6. Provenance and licences

Every file not written for the rewrite has a row in `PROVENANCE.md` (source, version,
licence, verbatim/modified/rewritten/generated), and material cargo cannot see has its licence
in `THIRD_PARTY/` (the Go implementation, CLDR via ICU4X, emoji data, Chroma (lexers, styles) and regexp2,
KaTeX, GoAT, smartcrop, gift, goldmark, flect, prose, Go's JPEG writer and decoder IDCT, Go fonts,
x/image, rsc.io/qr, hashstructure, livereload-js, and the Lato font of A-D3's test data).
`tools/dev/licence-check.sh` checks every crate of the dependency graph against
`deny.toml`; `tools/dev/notices.py <target> <file>` writes the notices of the linked
crates for a release (a crate without a licence file gets the MIT or Apache-2.0 text when that
is one of its licences, anything else fails). No Zola code: Zola ≥ 0.22 (EUPL-1.2) was never
opened; no pre-0.22 MIT Zola file was copied either (D2 allowed it; none was needed).

T70 audit: 535 third-party packages pass the licence check, 438 linked packages have notices;
no Go-source, "Copyright" or Zola text in the sources besides the attributed ports listed in
PROVENANCE.md. Fixed: `THIRD_PARTY/emoji/` was listed but missing (added, with the Unicode
licence and gemoji's MIT licence; the latter written offline, to be compared with gemoji's
`LICENSE`); rows added for `sites/**`, `testsite-go.txtar` and `testdata/golden/**`.

## 7. Known deviations from the Go implementation

Decisions (REWRITE_PLAN.md §11 D4, D5): Tera-only templates with v0.146 names; content is
rendered before layouts (another page's content inside a shortcode goes through
`page_content`); explicit pagination, a conflicting second `paginate` is an error; lazily
published resources are published on reference; no `#ZgotmplZ`; YAML 1.2 (`yes` stays a
string); deterministic winners for URL collisions; newer CLDR collation; no v1 shortcodes
(the legacy in-template version declaration); segment-aware prefix lookup; target-path assets: earlier language wins; site
functions inside components need `page=` or `@__nh`; `build.version` is fugo's version; the
template object is `build` and the environment variables are `FUGO_*` (below).

Allowed output differences (REWRITE_PLAN.md §7.3): minifier bytes, highlight span structure,
typographic characters vs entities, CSS/JS bundle bytes, term-collision winners, the order of
equal-weight Thai titles, the `generator` meta.

Per site (the ratchet's changes files, `tools/dev/changes/`):

| Site | Entry | Class |
|---|---|---|
| docs-i01, docs-reduced, docs-live | the Go build's stats file (`project:*`): not compared, fugo writes none (until T75: `<?xml` and `<=` are not tags, engine-difference) | accepted-deviation (T75) |

The earlier docs entries (a table on lazy list-item lines, the GoAT pages drawn by svgbob, the
math pages as MathML) are `bug-fixed` in T74: goldmark's table transformer, bep/goat and KaTeX
are ported.

Per crate (each README's "Accepted deviations" section, and `expected_diffs.toml` where the
tests read them, with counts):

- **base**: YAML `.inf`/`.nan` are strings; `BaseUrl` rejects non-UTF-8 paths, ports are
  `u16`; `humanize` of text without words returns the input; custom inflection errors are
  `Result`s; globs follow the documented semantics, not gobwas's optimiser bugs.
- **config**: no download of the Go implementation's modules (themes from `themesDir`, `_vendor`,
  absolute paths);
  imaging, media types and output formats per project, not per language; `deployment`,
  `segments`, `httpCache`, `server` untyped; errors instead of silently ignored values; no
  mapstructure weak decoding.
- **pageparser**: a summary divider at the start of a page without front matter is a divider;
  JSON integers stay integers; YAML 1.2; TOML leap seconds stay strings; closing tags must name
  their shortcode; unclosed inline shortcodes are errors; Org front matter not decoded.
- **vfs**: normalised `original` names; on macOS NFC names in `rel` but the OS's name in
  `abs` (Go normalises both); byte-ordered walks.
- **page**: attribute expansion in place; Unix-second dates in UTC; bad cascade globs are
  errors; content adapters are Tera `_content.html` (`_content.gotmpl` is an error with a hint),
  an unknown adapter page `kind` is an error; one menu entry per menu name.
- **site**: segment-wise taxonomy prefixes; bundle files belong to their owner; `ref` from a
  bundled page resolves; lists use the page's language's collator; strict `.Resources` order;
  main-section ties by name.
- **markup** (comrak): pipe tables are goldmark's paragraph transformer (a pass, not comrak's
  extension); the Go implementation's context markers shape the blocks but the closing marker after an include
  that ends with a table is not a row of empty cells, a marker inside code is removed; a fence
  without hook or highlighter renders plain; 2 typographer cases on docs pages not reproduced;
  no CJK line-break handling; integer attribute values.
- **highlight** (the Chroma port): Raku renders as one text token (its Go lexer rewrites its
  rules while running; not ported); Chroma's panics become `Error` tokens; where Chroma loops
  forever (zero-width matches returning to a configuration it had at the same position, e.g.
  JSONata, Jungle) the port treats the position as unmatched; `\p{…}` knows Go's general
  categories (crates/highlight/README.md).
- **images**: pixels by PSNR, not bytes; JPEG with an exactly rounded DCT; own `_hu_` names;
  smart crops are Go's (muesli/smartcrop with the Go implementation's analysis resize); stricter spec grammar;
  paletted PNG written true-colour;
  animated GIF first frame; simplified EXIF values; text and dither by PSNR.
- **resources**: Sass by grass (dart-sass semantics); minified bytes of ssg-minify; tools
  run in the project directory; `resources.Copy` conflicts are errors; own error texts.
- **publish**: canonify and stats-collector artefacts of Go's scanners not reproduced (counted
  in `expected_diffs.toml`); no generator injection; empty static dirs not copied.
- **layouts**: v0.146 names only (`index.*` refused); a user template does not beat a more
  specific theme template (the Go implementation's rule); base variants only for the selections loaded.
- **serve**: every change is a full rebuild (no fast render); no browser error page; `[server]`
  headers/redirects not read; per-language 404 pages; no TLS, `--openBrowser`, `--pprof`.
- **locale, nav**: `expected_diffs.toml` (CLDR 48 vs Go's x/text tables; pager and menu edge
  cases).

## 8. Open items

- **T73**: `ssg-migrate` (Go-template → Tera converter, stub today).
- **T72** (COULD): `:git` lastmod; Org front matter. (T74 did smartcrop, the Chroma style
  gallery and content adapters; the docs-live variant runs `images.Text`, `qr_code` and Dither
  unpatched. The i01/reduced patches stay as they are: they define A-D1 and A-D2.)
- **Server** (T71 leftovers): the browser error page, `[server]` headers and redirects, fast
  render (partial rebuilds), TLS, `--openBrowser`.
- **Performance** proposals of §5 (HTML minification); none is needed for the A-P goals.
- **A-D1** runs only through `compare.sh docs-i01`; a committed test like `gate_a_d2` would
  keep it green in CI.
- `ssg-config`: `TocConfig::end_level` is `u8`, so `endLevel = -1` cannot be decoded
  (markup README "Plan issues").
- `THIRD_PARTY/emoji/LICENSE-GEMOJI` to be compared with gemoji's `LICENSE` (written
  offline).
- The real-site tests that read `FUGO_SITES` are ignored by default.
- **Windows tests:** the Test job runs on Linux only, while the Go CI also ran its tests on
  `windows-latest` (`mage -v test`); Windows and macOS get only the release build and its smoke
  test. A Windows leg needs the Unix-only test code gated first: `use std::os::unix` in
  `crates/publish/tests/it/staticcopy.rs` (§9).
- The follow-ups of the Go removal outside the repository (§9): branch protection, unused
  secrets, the channels frozen at the last Go build.

## 9. Without Go (2026-10-01)

Branch `go-removal` (from `44529028`) removed the Go implementation and its CI/CD; the
repository is the Rust implementation only. `44529028` is the last commit with the Go tree (no
tag): `git worktree add <dir> 44529028`, or `git show 44529028:<path>` for the Go-tree paths
that comments and READMEs cite.

- **Removed:** the Go packages, `main.go` with `main_test.go` and
  `main_withdeploy_test.go`, `go.mod`/`go.sum`, `magefile.go` (and `.vscode/`, its debug
  configuration), the release (GoReleaser, the upstream project's own release tool with its `.env` file,
  `merge-release.sh`), Docker, snap and golangci-lint configuration, `check_gofmt.sh`,
  `watchtestscripts.sh`, `testscripts/`, `scripts/`, `tools/go-oracle/`,
  `tools/dev/oracle.sh`, `tools/esbuild/build.sh`, the highlight oracle
  (`rust/crates/highlight/tests/data/oracle/` at `44529028`), and the Go workflows `ci.yml`
  (Go's; the current `ci.yml` is the renamed `rust.yml`, below; Go's ran its tests on
  `ubuntu-latest` and `windows-latest`, the current one tests on Linux only, §8),
  `release.yml`, `benchmark.yml`, `golangci-lint.yml` and `image.yml`. `.github/stale.yml`
  (the Probot stale bot's configuration, with other labels and periods than the
  `workflows/stale.yml` that manages issues) is not Go's and stays, for the maintainers to
  decide on. `pull-docs.sh` (a `git
  subtree pull` of `docs/` from the Go version's docs repository) is gone too: the legacy docs
  site is a frozen test fixture now and is no longer pulled; it moved to `testdata/legacy-docs/` (with its `go.mod`,
  `go.sum` and Go workspace file, byte-identical) when `docs/` became fugo's own documentation
  (2026-10-02).
- **Test data moved:** the Go tree's test data the tests read is in `testdata/upstream/` at its
  Go-tree path (`testsite`, `resources/testdata`, `resources/images/testdata`,
  `tpl/images/testdata`, `media/testdata/fake.png`; 90 files). Fixture ids keep the old paths;
  `ssg_testkit::fixture::repo_file` resolves them (and `tools/rust-port/i01/sites.py` does
  the same for the testsite). The image oracles read five more Go-tree images and
  `snap/local/logo.png` from byte-identical copies (`crates/images/tests/it/common.rs`). The
  `FUGO_GOROOT` hook for Go's own image test data is gone: the 80 files of it the image
  oracles read (Go 1.24.7's) are in `testdata/upstream/goroot/src/image/`, and the five Go
  1.24.7 does not have (four Go 1.27.1 JPEGs and `image/png`'s example gopher) are the old
  port's copies from `be02933a` in `testdata/upstream/old-port/`. So the process oracle
  compares all 12,264 cases (12,262 equal, two accepted corrupt-PNG differences; CI at
  `44529028` with Go 1.24.7 compared 13,089) and the EXIF oracle 2,335; both fail when a
  source is missing.
- **`js_build` in process (2026-10-02):** `js_build` bundles with rolldown 1.2.12 in process
  (crate `ssg-jsbuild`, which replaced the esbuild `--service` client crate), so
  nothing needs installing and the release archives are complete. rolldown and its oxc are
  exact pins (`rolldown*` `=1.2.12`, the `oxc` umbrella crate `=0.152.0`, beside the oxc 0.95
  minify-html locks), which raised the MSRV to 1.96 (CI 1.96.0); the `vendor/sauron-core` patch
  this first needed (svgbob's sauron 0.61 pinned futures `=0.3.30`, rolldown needs `^0.3.32`)
  went with svgbob, which the GoAT port replaced (T74). The plugin (`src/plugin.rs`) keeps js.Build's semantics on top of rolldown:
  assets-first resolution with shims and esbuild's external matching, `@params`, `inject`
  (side effects included), CSS imports (the CSS is discarded as before; `local-css` gives class
  names), TC39 decorators and the `es5` target (`src/lower/`), `process.env.NODE_ENV`,
  `require()` for externals in IIFE output, and named CommonJS exports. A build is a task on a
  tokio runtime and the caller blocks on a channel (`src/executor.rs`): never wait on one from
  a worker of rayon's global pool, which rolldown itself uses. The output is not esbuild's
  bytes, so the oracle tests compare behaviour: `crates/jsbuild/tests/it/{jsbuild,run}.rs` run
  the Go build's recorded bundle and this port's under node with recording stand-ins for the browser
  and compare the traces; errors keep Go's positions (and esbuild's wording for unresolved
  imports and `es5`). Fingerprints and `Data.Integrity` of bundles changed with the bytes.
  `tools/esbuild/` and the npm pin are gone; a checkout made before needs
  `tools/dev/node.sh` once (the lock file changed). rolldown also turns on serde_json's
  `preserve_order` and `arbitrary_precision` for the whole binary; the workspace-hack turns
  them on for every member and DEVELOPMENT.md ("Feature unification") has the two rules that
  follow (no reliance on `serde_json::Map` order; no `serde_json::Value` numbers through Tera,
  YAML or TOML serializers).
- **Frozen references:** what the Go implementation generated stays as committed: the oracle
  fixtures (`testdata/oracle/`), the golden data (`testdata/golden/`),
  `crates/build/tests/it/testsite-go.txtar`, the highlight fixtures
  (`crates/highlight/tests/data/`) and lexer table
  (`crates/highlight/src/data/chroma-lexers.tsv`), `crates/funcs/tests/fixtures/remarshal/go.txt`
  and `testdata/legacy-docs/data/docs.yaml`; so do the Go outputs the old port recorded at `be02933a`, such as
  `testdata/corpus/minify/*.tsv` (PROVENANCE.md). To regenerate, run the old recipe in a
  worktree of `44529028` and copy the result back (`testdata/golden/README.md`,
  `crates/highlight/README.md`, `tools/dev/fixtures2json.py`). `compare.sh` takes only
  `--ref golden`; `selftest.py` perturbs the Go testsite output of `testsite-go.txtar`.
- **Workspace at the root:** the Cargo workspace moved from `rust/` to the repository root
  (`Cargo.toml`, `crates/`, `sites/`, `testdata/`, `THIRD_PARTY/`, `PROVENANCE.md`; build output
  in `target/`); `rust/README.md` became `DEVELOPMENT.md`, `rust/docs/template-api.md` this
  directory's `template-api.md`, and the workflow `rust.yml` became `ci.yml` (name `CI`). Ids
  recorded below `rust/` (the sources of `testdata/golden/images/manifest.json`) resolve at the
  root through `repo_file` (`ssg_testkit::fixture`, `sites.py`).
- **Releases:** tags `v<version>` instead of `rust-v<version>`; the CI workflow publishes the
  GitHub release (a pre-release if the version has a `-`, else latest only if no release has a
  higher version). The Go releases (`v0.148.2` and older) and their tags were removed on
  2026-10-04, when `v1.0.0` was released.
- **Drop-in names:** the binary has the Go version's name again (it had an `-rs` suffix; since
  the rename, §10, it is `fugo`); `fugo version` prints the Go line,
  `fugo v<version>[-<commit>] <os>/<arch> BuildDate=<date|unknown>[ VendorInfo=<vendor>]`
  (`crates/cli/src/version.rs`; CI sets the commit, its UTC date and `VendorInfo=fugo` as
  the Go releases' build did); the archives are named as goreleaser named them,
  `fugo_<version>_<os>-<arch>.tar.gz` (`.zip` for Windows), with the binary, `README.md` and
  `LICENSE` at the root plus `THIRD_PARTY_NOTICES.txt`, `PROVENANCE.md` and `THIRD_PARTY/`,
  next to `fugo_<version>_checksums.txt`. `compare.sh` takes the binary from
  `FUGO_BINARY` (was `FUGO_RS`). Not reproduced from the Go release: goreleaser's
  changelog in the release notes and its `v<version>` release title (the title is now
  `fugo <version>`); the Go commands the Rust command line does not have (`env`, which also
  printed the version line, `new`, `mod`, `deploy`, `list`, `gen`, `convert`, `import`,
  `release`, `server trust`, `config mounts`, cobra's `completion` and `help`), the Go program's flags it
  does not list (`crates/cli/README.md`; e.g. `--enableGitInfo`, `--contentDir`,
  `--disableKinds`, `--panicOnWarning`, the build's `-w`/`--watch`, the logging and
  housekeeping flags `--gc`, `--logLevel`, `--noBuildLock`, `--printI18nWarnings`,
  `--printPathWarnings`, `--printUnusedTemplates`, `--templateMetrics` and
  `--templateMetricsHints`, and the server's `--disableFastRender` and `--disableBrowserError`;
  `--noTimes`/`--noChmod` work), the "Start building sites …" banner with the version line, and
  Go's per-language statistics table after a build (Pages, Paginator pages, Non-page files,
  Static files, Processed images, Aliases, Cleaned): fugo prints one line, `pages N | files N
  (aliases N) | resources N | processed images N | static files N`, then `Total in N ms`;
  `config`'s output: Go printed one language's configuration with its keys lower-cased
  (`baseurl`, `publishdir`, …), as TOML by default or YAML/JSON, with `--lang` and
  `--printZero`; the Rust `config` prints fugo's resolved configuration model (snake_case
  fields, one entry per site under `sites`, the merged user keys lower-cased under `raw`) as JSON
  or TOML, so a script that reads its output must change; `server`
  rendering to disk by default: the Go server wrote the publish directory and served it
  (`-M`/`--renderToMemory`: memory), the Rust one renders into memory unless `--render-to-disk`
  (a fugo flag, not the Go build's) is given, and `-d` needs that flag; the commit and date
  of a local build (Go read them from git, a cargo build gets them only from CI's variables);
  the exit code 1 of usage errors (they exit with 2) and Go's `Error: …` prefix (clap prints
  `error: …`, a failed build `ERROR …`). As in Go, flags may come before the command and every
  command takes the persistent flags (`-s`, `-d`, `-e`, `--config`, `--configDir`, `--themesDir`,
  `--clock`, `--quiet`, `-M`; `crates/cli/README.md`). New: a `--version` flag, printing the
  `version` line. The Docker images and the docs deploy are below; `snap/snapcraft.yaml` and
  the upstream release tool's configuration were in the tree at `44529028`, but no workflow published them.

Follow-ups outside the repository:

- **Branch protection:** required status checks that name jobs of the removed workflows (CI's
  `Build (ubuntu-latest, Go 1.25)`, Golangci-lint, Release, Benchmark, Docker image) would block
  every pull request; require the CI workflow's `Lint`, `Test` and `Build (<target>)` instead.
- **Secrets** no workflow reads any more: `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN`, `CR_PAT`
  (the Go build's `image.yml`; fugo's `image.yml` pushes to ghcr.io with the job's token),
  `FUGO_GITHUB_TOKEN`, `FUGO_EMAIL` (the docs deploy of `release.yml`).
- **Public channels frozen at the last Go build:** the Go version's Docker images (Docker Hub and
  ghcr.io, under the former name; fugo's image is `ghcr.io/getfugo/fugo`); and the
  documentation site getfugo.github.io (deployed on `v*` tags by `release.yml`).

## 10. History

- 2026-09-27: the first session's byte-for-byte port (`archive/HANDOFF-old-port.md`).
- 2026-09-29: REWRITE_PLAN.md revision 2; T00 deleted the old port.
- T00–T66: the rewrite, the harness and the gates (REWRITE_PLAN.md §8.2).
- T70 (2026-09-30): this handoff, A-P, the licence and provenance audit, archive of the old
  port's documents, `is_server` (now `build.is_server`)/`site.server_port`.
- 2026-10-01: the Go implementation and its CI/CD removed, the Cargo workspace moved from `rust/`
  to the repository root, the binary, its version line and the release archives named as the Go
  releases named them (§9). Then: `to_css` resolves explicit-extension imports (`@import
  "x.scss"`) through the load paths as dart-sass does; the template object and the
  environment variables named after the Go program renamed after the project (then the Go
  version's name), without fallback, and every other name of the Go program (config file, stats
  file, cache dir, generator, …) made the project's (§2);
  the owner's real site ported to Tera and served (outside the repository).
  Engine fixes found there: `[build] writeStats` was not migrated (no stats file, so PostCSS
  purged the whole stylesheet); CSS minification no longer runs lightningcss's merging pass,
  which dropped fallback declarations (`background: radial-gradient(…); background:
  -webkit-radial-gradient(…)`); `<title>` whitespace is collapsed as Go's minifier does. CSS
  browser targets switched on: a project browserslist configuration (`.browserslistrc`,
  `browserslist`, `package.json` key; `crates/minify/src/targets.rs`, browserslist-rs) gives
  lightningcss targets, so minified CSS gets the prefixes and lowering autoprefixer would add;
  rules declaring a property twice are rescued as written. `execute_as_template` passes `data`
  to the template as the Tera value it is (the `TemplateExecutor` holds the context) instead of
  converting it to JSON and back on every call: the owner's site's development head, which executes
  its stylesheet with `data=page` on every page, built in 12.4 s with 3.1 GB, now 1.8 s with
  220 MB. Held outputs (pages waiting for `post_process` / `defer` placeholders) are written to
  the sink unpatched and patched there in E5 (`Sink::read`), as the Go implementation's post-processing does,
  instead of being kept in memory: the owner's site's production build, where the inlined
  post-processed stylesheet holds every page, went from 386 MB to 234 MB, output unchanged.
  `purge_css` (fugo's own, no Go equivalent; `crates/minify/src/purge.rs`): a CSS
  resource cut down per page to the rules that page uses, PurgeCSS-style options (`safelist`,
  `greedy`, `blocklist`, `content`, `variables`, `important`), compiled once (each selector and
  declaration printed for the browserslist targets) and resolved by the publisher from the
  page's own HTML; no stats file, no PostCSS, no held pages. A pending `fingerprint` is
  computed at the call unless its chain waits for E5 (PostCSS, Tailwind, images). The owner's
  site now uses `to_css | minify | purge_css` (PostCSS, PurgeCSS, cssnano and `writeStats` removed):
  median inline CSS 54 KB → 38 KB, all HTML −17 % (−14 % gzipped), screenshots of six page
  types identical except the carousel indicators, whose attribute-selector rule PurgeCSS had
  dropped.
- 2026-10-02 (T74): the documentation site built by fugo matches the published
  https://getfugo.github.io/ (the Go build of 2025-10-13) on every page: gate A-D3
  (`gate_a_d3`, `compare.sh docs-live`) compares `docs/` without patches (`sites.py make
  docs-live`) with the published files (`testdata/golden/docs-live/`, getfugo.github.io at
  a1928152), at that build's clock, with that day's GetRemote responses
  (`tools/rust-port/testdata/getremote-cache/docs-live/`): L1 2373/2373, L2 776/776, L4 873/873,
  A7 1.0 (the one L3 difference is Go's stats tokenizer reading `<?xml`/`<=` as tags), every
  highlighted code block and the style gallery byte-identical. `tools/docs/build.sh [--serve]`
  (now `tools/legacy-docs/build.sh`) builds (or serves) the site with the docs' node modules
  (`tools/docs/package-lock.json`, now `tools/legacy-docs/package-lock.json`, the
  versions the published JS bundles were built with). Engine work: `get_remote` sniffs media
  types with Go's whole table; content adapters as Tera `_content.html` (`add_page`,
  `add_resource`, `enable_all_languages`; the news adapter ported); `to_math` runs KaTeX 0.16.22
  with mhchem in QuickJS (rquickjs) as the Go implementation does (pulldown-latex removed); `diagrams_goat` ports
  bep/goat v0.5.0 (svgbob removed); the `smart` anchor ports muesli/smartcrop with the Go
  implementation's analysis resize; pipe tables are goldmark's paragraph transformer and `{{% %}}` output is
  indented as the Go implementation does; highlighting ports Chroma v2.19.0 (syntect and two-face removed).
  A-D1 and A-D2 reach A7 1.0 as well (their GoAT, math and table entries `bug-fixed`).
- 2026-10-02: renamed **fugo** (GitHub org `getfugo`; repository `getfugo/fugo`, website
  https://getfugo.github.io/), and the source code avoids the program's name. The name is written
  once, in `ssg_base::app_name!` (`APP_NAME`; `env_prefix!`, `env_var!("X")` for the
  `FUGO_*` variables), besides the binary's name in `crates/cli/Cargo.toml` and `APP_NAME` in
  `.github/workflows/ci.yml`; the version line, the generator tag, the cache directory, the
  LiveReload server name, GetRemote's user agent, the release archives (package.py and
  notices.py read the binary's name) and the harness scripts derive it. Generic names
  elsewhere: crates `ssg-*` (package `ssg-cli` builds the binary `fugo`), `tools/dev/` (was
  a directory named after the project), the configuration file `config.*` only (the
  project-named file is not read), the
  template object `build` (`@build`; Sass `build:vars`), the stats file `build_stats.json`, the
  reserved layouts directory `_internal/`, `package.project.json`, internal ids `ssg-*`, the
  data schemas `ssg-structure/1`, `ssg-manifest/1`, `ssg-baseline/1`, `ssg-docs-patches/1`.
  Comments say "this port" or "native" where they named the project. Historical documents
  (`archive/`, the ratchet records in `tools/dev/changes/`, and this history) kept the
  names they had until 2026-10-03 (below).
- 2026-10-02: **the owner's private site removed from the repository** (the leaked YouTube API
  key it carried was redacted from the whole history of `rust-port`, force-pushed; older commits
  still hold the rest of its data): the reconstruction site and gate A-R with their golden data,
  baseline and GetRemote cache; the golden-build tooling (`tools/rust-port/{prepare-site,
  build-site}.sh`, `compare.py`, `golden/`); the site's Go-oracle fixtures; the goldmark, YAML,
  time, minify and collation corpora made from its pages; its images and the oracle cases that
  used them; the research specs (`docs/rust-port/specs/`, which analysed its templates); the
  CLI oracle's cases run in its configuration (`oracle/commands/cli`), the minifier corpus's
  fuzz rows (mutations of its pages) and inline scripts, and the flect results of its name.
  Synthetic fixtures that used its name as a label or base URL now say `seasample`,
  `snack.example` or `site`; the CSS chain tests use `crates/resources/tests/fixtures/styles.txtar`;
  three images of the synthetic resources site were replaced by generated ones (their
  content-derived expectations updated); the mini e2e case's YouTube response has neutral
  text. Gate A-S (the real site) is no longer a repository gate.
- 2026-10-03: the repository no longer names the Go program whose behaviour fugo reproduces,
  the historical documents included: they say "the Go implementation" (or Go), "the Go
  version" (the fork this rewrite replaced) and "the legacy docs site" (the Go tree's `docs/`).
  Paths renamed: `testdata/legacy-docs/`, `tools/legacy-docs/`, `testdata/oracle/sitebuild/`,
  `testdata/upstream/testsite/`, `tools/rust-port/testdata/getremote-cache/`,
  `archive/LAYER_CRITIQUE.md`. Literals that must match recorded data (the golden manifests' key
  of the Go build's stats file, the file and variable names the Go builds used) stay, as named
  constants in the harness scripts.
- 2026-10-03: **no PostCSS, no `FUGO_ENVIRONMENT`.** The `postcss` filter, its pipe, its oracle
  fixture and the `postcss`/`postcss-cli` node tools are gone (`minify` adds the browserslist's
  vendor prefixes, `purge_css` purges per page; a template calling `postCSS` gets the migration
  hint). The default `security.exec.allow` and cache buster no longer name postcss, and
  `postcss.config.js` is not mounted under `_jsconfig` (the config and vfs oracle tests map
  those recorded defaults back). The environment comes from the command (`--environment`;
  `production`, `development` for `server`) and no variable chooses it or reaches the tools;
  `.env.<environment>` is read after `.env` and wins, and `fugo server` reloads on either.
- 2026-10-03: **no Babel; fugo 1.0.0.** The `babel` filter, its pipe and tests, its docs page
  and the `@babel/*` node tools are gone (`js_build` compiles TypeScript and JSX and lowers
  modern JavaScript; a template calling `babel` gets the migration hint); `babel.config.js` is
  not mounted under `_jsconfig`. The workspace version is `1.0.0-DEV` (releases start at
  `v1.0.0`: the Go fork's tags `v0.1.0` … `v0.148.2` stay in the repository), `build.version`
  is fugo's version (`build.app_version` is gone), and the legacy docs site's layouts print the
  recorded Go build's `0.149.0-DEV` themselves. The object reference no longer appends
  `(Go: …)` to each field.
- 2026-10-03: **a Rust-only asset pipeline (T75).** The Tailwind pipe, the tool machinery
  (tool lookup, `exec`, the tools' configuration-file mounts and cache busters, CSS `@import`
  inlining for tools), the embedded JavaScript runtime (`deno_runtime`, V8, N-API, the hidden
  `__run-package` command) and the stats file (`[build.buildStats]`, `build_stats.json`, phase
  E4, the store's generated assets, the serve watcher's exception) are gone; `[build]
  buildStats`/`writeStats` are "no longer supported" warnings and `[security.exec]` has no
  effect. The npm installer stays (`js_build` and Sass read `node_modules`). `HtmlElements`
  stays for `purge_css`, still checked against Go's collector oracle. The legacy docs site's
  Tailwind stylesheet is recorded (`sites/docs/assets/css/styles.tailwind.css`) and published
  by the overlay, so the gates' files are unchanged; the harness leaves the Go build's stats
  file out (A-T 55, A-D1 887, A-D2 888, A-D3 2372 files; `tools/dev/changes/T75.md`). The
  release binary went from 227 MB to 87 MB (71 MB stripped), `Cargo.lock` from 1,238 to 780
  packages, and encoding_rs, rustls and rand are no longer held back. The docs' function pages
  no longer print each function's Go-template name.
- 2026-10-03: **versions come from git tags.** A release is the tag `v<major>.<minor>.<patch>`
  (`tools/dev/version.py`); CI builds it with `FUGO_BUILD_VERSION` (read at compile time by
  `ssg_base::VERSION`, which `fugo version`, `build.version`, the generator tag and the npm
  installer's user agent use), and `Cargo.toml`'s version is `0.0.0-DEV`, the version of builds
  not made from a tag. The **Bump version** workflow (`.github/workflows/bump.yml`,
  `workflow_dispatch` with `version_bump` major/minor/patch) tags the branch head with the
  next version (`v1.0.0` first; the Go fork's v0.x tags do not count) and dispatches CI on the
  tag, whose release job publishes it (DEVELOPMENT.md "CI and releases").
- 2026-10-04: **v1.0.0 and the container image.** The Rust port was merged into `main` (#140)
  and released as `v1.0.0`; the Go fork's 361 tags and 40 releases were removed from the
  repository. `Dockerfile` packages a release's Linux binary on Debian slim (it downloads the
  release archive, it does not compile), and `.github/workflows/image.yml`, dispatched by the
  Release job, pushes it to `ghcr.io/getfugo/fugo` for linux/amd64 and linux/arm64
  (DEVELOPMENT.md "Container image").
