# fugo Rust rewrite plan

**Status.** Final plan, revision 2, dated 2026-09-29. This revision applies the completeness, feasibility and Rust-style reviews. Review points that were rejected, or only partly applied, are listed in §12 "Review notes".

**Since the Go removal** (after `44529028`, 2026-10-01) the Cargo workspace is the repository root and the binary is `fugo` ([DEVELOPMENT.md](../../DEVELOPMENT.md), [HANDOFF.md §9](HANDOFF.md#9-without-go-2026-10-01)). The `rust/…` paths and `rust/target` below are the layout this plan was written for: read `rust/<path>` as `<path>` (`rust/README.md` is now `DEVELOPMENT.md`, `rust/docs/template-api.md` is `docs/rust-port/template-api.md`). The plan was written under the Go version's names and now uses today's: the binary and the CLI package are `fugo` (when the plan was written, the binary had the Go version's name with an `-rs` suffix, and the CLI crate's directory, today `crates/cli`, had the Go version's name), the template object is `build` (it had the Go program's name), and the code sketches use today's names (`BuildView`, `StatsFile`, `FuncSpec::go`). Where they differ, DEVELOPMENT.md and HANDOFF.md win.

**Since the private site was removed** (2026-10-02) the repository no longer holds the owner's private site: its reconstruction (R, gate A-R), gate A-S, its corpora and golden data, and the old port's specs (`docs/rust-port/specs/`) are gone. The plan calls it "the private site" and keeps its tasks as written; [HANDOFF.md](HANDOFF.md) has the current gates.

**Since the asset pipeline became Rust only** (T75, 2026-10-03) there is no Tailwind, PostCSS or Babel pipe, no embedded JavaScript runtime and no stats file (`StatsFile`, phase E4): the plan's tasks and gates keep them as written; [HANDOFF.md](HANDOFF.md) has what exists.

**What it replaces.**
- `crates/TERA_PLAN.md`. T00 moved that file to `docs/rust-port/` with a pointer to this plan (T70 archived it: `docs/rust-port/archive/TERA_PLAN.md`).
- The byte-parity rules in `crates/README.md`.
- `docs/rust-port/HANDOFF.md` §4 (T70 rewrote the handoff; the old one is `docs/rust-port/archive/HANDOFF-old-port.md`).

**Repo.** A checkout below `/home/user`, branch `rust-port`.

**Facts checked while writing this plan.**
- **Disk.** 4.5 GB free. `crates/` takes 423 MB, `~/go/pkg/mod` takes 2.2 GB. The working tree without `crates/` and `.git` is 55 MB.
- **Docs layouts.** `docs/layouts` has 76 files and 3,372 lines. `docs/layouts/_shortcodes/quick-reference.html` prints `.Content` of other pages.
- **Version.** The fork is `0.149.0-DEV` (the Go tree's `version_current.go`).
- **Private-site reconstruction.** It has `_default/_markup/render-table.json.json`, and its home, list, term and 404 layouts call `.Paginator`.
- **Tera 2.4.0 behaviour:**
  - A list comprehension accepts a single `if` clause (`parser.rs` `parse_list_comprehension`).
  - `starting_with` and `ending_with` take `pat=` (`tests.rs:147-158`).
  - The `fast` feature equals `no_fmt` + `fast_escape` + `fast_hash`.
  - A component renders with a fresh `State`. It sees only its arguments, plus `@implicit` values looked up through the caller chain (`vm/interpreter.rs:965-1000`, `vm/state.rs:113-160`).
  - A site function's `State::get` never reaches the caller of a component.
- **Toolchain.** rustc and cargo 1.94.1.
- **I01 patches.** `tools/rust-port/i01/sites.py` `DOCS_REMOVE` / `DOCS_REPLACE` / `DOCS_WRITE` (lines 88-150) patch layouts as well as content and config. The patches include `codeFences = false`, Tailwind → `minify`, QR/Text → Resize/Grayscale, and the Chroma → `printf` replacements.

---

## 1. Summary and decision

### 1.1 Decision record

| Topic | Decision |
|---|---|
| Approach | A new Cargo workspace `rust/` with 22 product crates, a dev-only `testkit`, and an internal `workspace-hack`. The Go implementation's site and page model is rewritten in idiomatic Rust. We do not fork Zola, and we do not refactor the old crates in place. |
| Old port | T00 tags the current byte-identical state as `go-parity-final`. It converts the engine-neutral fixtures to plain JSON, moves them to `rust/testdata/`, and deletes `crates/` completely, ignored `target/` directories included. Later tasks read salvage material with `git show go-parity-final:crates/...`. |
| Oracle | The Go build of the same `sites.py` inputs is the structural oracle. It gives the same files, URLs, aliases, feeds, chosen templates and resource URLs. Output bytes no longer have to match. |
| Templates | `tera = "=2.4.0"` with features `no_fmt`, `fast_hash`, `preserve_order`; `fast_escape` is not used. <ul><li>Layouts, and assets processed with `execute_as_template`, are hand-converted into `rust/sites/<site>/`.</li><li>The engine accepts **only the Go implementation's v0.146 (new-style) layout names**.</li><li>Content files keep the Go implementation's shortcode syntax and never go through Tera.</li><li>i18n files keep the Go implementation's `{{ .Field }}` placeholders, read by a restricted evaluator (not a template engine).</li></ul> |
| Template API | Tera-native: operators, list comprehensions with `if`, `~`, map and array literals, components. There is no Go `printf`, `where` or `Scratch` clone. The single source of truth is the Rust table `ssg_funcs::spec::FUNCS`. `rust/docs/template-api.md` is generated from it and snapshot-checked. |
| Markdown | comrak 0.55 behind an engine-neutral `ssg-markup` API. The T04 spike runs it over all 959 docs files and all 251 private-site bodies before the API is frozen. pulldown-cmark is the fallback behind the same API. |
| Highlighting | syntect 5.3 + two-face, emitting Chroma class names (and inline styles for `noClasses`). Not giallo, which is EUPL-1.2 (D1). |
| Data model | An arena of `Page` values in `IdVec<PageId, Page>`, with one `BTreeMap<ContentKey, PageId>` tree per language. URLs are computed after cascade. Every path flavour is a newtype. Data files and nested values use a case-preserving `Map`; front matter, config and language params use the case-folded `Params`. |
| Views | Values are pre-serialised once and shared through `Arc` as `tera::Value`. Relation lists hold **summary** values; `deref` and `get_page` return full values. There is a Meta generation (content phase) and one Full generation per hook variant. All generations are frozen in `OnceLock`s before any layout renders. |
| Render scope | Every render (layout job, shortcode, hook, `partial()`, `render_string`, `defer`, `execute_as_template`) carries an explicit scope value `__nh` in its Tera context: page, language, format, pager, phase, variant, frame, transaction, depth and call chain. There are **no thread-locals**. |
| Content | All content is rendered before layouts. Memo cells never block. A cycle is detected through the call chain in the scope. Fragments form their own memo stage. Page-store writes made during content rendering are buffered and committed only by the computation that wins its memo cell. |
| Pagination | `paginator()` and `paginate()` record the first call per (page, format). An identical re-call reuses the recording. A re-call with a different list or size is an error that names both template positions. |
| Publishing | Bundle resources are published eagerly. Every other resource is published when its URL appears in a rendered template output, matched after decoding HTML and JSON escapes, or when the `publish` filter is applied. There is no scan of plain CSS or JS. |
| Verification | <ul><li>A walking skeleton in round 7.</li><li>A Go **structure oracle**: per (page, format) the template and baseof, targets, permalinks and resource URLs.</li><li>A stdlib-only Python `structdiff` with levels L1–L4, a minified and an unminified pass, and a ratchet baseline with triage classes.</li><li>Two docs patch variants, mirrored 1:1 in Go and Tera.</li></ul> |
| Agent isolation | One git worktree per agent. All worktrees share `CARGO_TARGET_DIR=rust/target`. Only green commits are merged. |
| Licence | fugo stays Apache-2.0. The policy lives in `rust/deny.toml` (cargo-deny format) and is evaluated as SPDX expressions by `tools/dev/licence-check.sh`. No EUPL, GPL or AGPL. Zola 0.22 and later is a design reference only (D2). |

### 1.2 What "Rust style" means here (binding for every task and review)

**Typed domain model**
- Use enums instead of bundles of bools: `UglyPolicy`, `Escaping`, `PageRole`, `Markup`, `BlockquoteKind`, `Alignment`, `InnerUse`, and so on.
- Every path flavour is a newtype with exactly one constructor: `ContentKey`, `TermKey`, `OutputPath`, `UrlPath`, `Permalink`.
- Typed ids index `IdVec<I, T>`. There is never a `.0 as usize`.
- Only user params, data files and markup attributes are untyped (`Value`).
- The clippy pedantic subset includes `struct_excessive_bools` and `fn_params_excessive_bools`.

**Ownership and state**
- The `Model` is built once, then shared read-only through `Arc`.
- Mutable build state lives only in named concurrent collections owned by the render `Session`:
  - content memo cells;
  - page stores;
  - the pagination recorder;
  - the deferred registry;
  - partial frames;
  - the resource registry;
  - the image queue.
- There are no global mutable statics and **no thread-locals**. `rust/clippy.toml` sets `disallowed-macros = ["std::thread_local"]`.
- The render position travels as an explicit scope value.

**Services**
- Each Tera function is a small struct that holds only the `Arc`s it needs, for example `GetPage { model, views }` or `I18n { translations }`.
- Only content-dependent functions hold a `Weak<dyn ContentRenderer>`. That trait has 5 methods.
- There is no service locator.

**Errors**
- Every library crate uses `thiserror` enums, including `ssg-build` (`BuildError`). `anyhow` is used only in `cli`.
- Site functions wrap causes with `tera::Error::chain`.
- Diagnostics carry a `Position`. They are aggregated, de-duplicated and sorted before they are reported.
- We never reproduce Go error texts.

**Determinism**
- Determinism comes from sorted collections and explicit tie-breaks.
- We never emulate Go map order, sort instability or worker-count effects.
- We never reproduce quirks of the Go implementation that come only from Go details: `sync.Once` pagination, character-level radix prefixes, the v1 shortcode version declaration, printf verbs.

**Config**
- One `Value`-tree pipeline (normalise → migrate legacy keys → merge once → per-language merge), then typed serde structs with `Default`.
- No mapstructure-style decoders.

**Parallelism**
- rayon over natural units: files, pages, jobs, image ops. No async in the build path.
- A render never starts parallel work. This is checked with a debug assertion on `render_pool.current_thread_index()`.

**Crate hygiene**
- `#![forbid(unsafe_code)]`.
- Small public APIs with rustdoc.
- Heavy dependencies isolated in leaf crates.
- `cargo fmt` everywhere; clippy (`all` + the reviewed pedantic subset) once per phase.

**Go is reference only.** Go files, the old port, its specs and the oracles are read for rules and test cases. Every task review checks:
1. no Go-order emulation;
2. no `interface{}`-style or stringly-typed plumbing;
3. no Go method tables;
4. no byte-level Go state machines transliterated;
5. no Zola-isms (Section/Page split, URLs computed at parse time, `@/` links, orphan pages).

### 1.3 Why this base, and what was grafted

**Why not the alternatives.**
- **zola-derived was not chosen.**
  - Only about 15% of its lines come from the MIT fork point, and that is Tera 1 without the modern Zola pipeline.
  - It chose pulldown-cmark to stay close to the fork, not because it fit.
  - It missed the cross-page `.Content` and per-format hook needs.
  - Its lazy cells block, which can deadlock under rayon.
- **incremental-refactor was not chosen.**
  - It carries two template engines and `go_value` shims.
  - After wave 5 it becomes a long serial tail, and its disk use peaks while old and new code coexist.
  - Its verification ideas were adopted instead.

**Grafts.**

| Graft | Source | Where |
|---|---|---|
| Tag `go-parity-final`, delete legacy code, salvage through `git show` | zola-derived | §6, T00 |
| Go `structure` oracle | zola-derived | T01; gates in T23b, T30, T60–T66 |
| Summary relation lists, `deref`, Meta/Full generations | zola-derived | §2.5, T33 |
| structdiff L1–L4, allowlists, staged docs gates | zola-derived | §7 |
| `_embedded/` and `_theme<N>/` fallback prefixes | zola-derived | §4.1 |
| `Hooks` trait with default methods | zola-derived | §2.4 |
| Licence check, `PROVENANCE.md` | zola-derived | §5, T00 |
| Ratchet with triage classes, harness self-test, unminified pass | incremental-refactor | §7.2, T03 |
| comrak spike in round 2 | incremental-refactor | T04 |
| `templates check` | incremental-refactor | §4.8, T37 |
| Walking skeleton | incremental-refactor | T38 |
| Pagination recorded at call time | incremental-refactor | §3.3 |
| Explicit render scope, frames by id, chain-based cycle detection | Rust-style review | §2.5, §4.2 |
| Case-preserving `Map` vs folded `Params` | completeness and feasibility reviews | §2.4 |
| Docs patch variants 1:1 with Go | completeness and feasibility reviews | §7.4 |

### 1.4 Accepted weaknesses

1. **The Go implementation's semantics are re-derived**: cascade, sanitising, taxonomy keys, layout scoring, pagination. Mitigation: salvage from `go-parity-final`, with oracle fixtures from day one.
2. **The first real full build comes in round 10.** The walking skeleton (round 7) and the structure-oracle gates (T23b, T30) give earlier signals.
3. **Existing sites of the Go implementation do not run unchanged.** Every layout, and every asset used as a template, must be converted to Tera (D4). That is about 5k Tera lines for the three sites, plus about 1.2k lines of embedded templates.
4. **Acceptance is structural, not `cmp`.** It is backed by the ratchet and hard invariants (§7).
5. **Some Go-template idioms become explicit calls.**
   - Another page's `.Content` inside a shortcode becomes `page_content(page=…)`.
   - Inside a component, site-bound functions need `page=` or the implicit `@__nh` parameter.
   - A second, conflicting `.Paginate` becomes an error.

---

## 2. Workspace layout

### 2.1 Directory tree

```
rust/
  Cargo.toml  Cargo.lock    # [workspace] resolver="3", edition 2024, rust-version "1.94"; ALL third-party deps+features in [workspace.dependencies]
  .cargo/config.toml        # [build] target-dir = "target", jobs = 4
  clippy.toml               # disallowed-macros = ["std::thread_local"], pedantic subset config
  deny.toml                 # licence allowlist + bans (cargo-deny format; read by licence-check.sh)
  README.md                 # agent rules (§8.1), lanes, worktrees, how to run acceptance
  PROVENANCE.md             # every non-original file: source, commit, licence, verbatim|modified|rewritten
  THIRD_PARTY/              # the Go implementation's Apache-2.0 licence (embedded templates), asset licences cargo cannot see (two-face syntaxes/themes, CLDR, emoji data, livereload.js)
  docs/template-api.md      # GENERATED from ssg_funcs::spec::FUNCS + context tables; insta-checked
  crates/
    base/        ssg-base        ids+IdVec, PageKind/KindSet, Value/Map/Params/Date, path newtypes, anchors, inflect/title, globs, time, diag, Collate, Sink
    config/      ssg-config      Value-tree config pipeline, per-language SiteConfig, output formats, media types, security, caches, privacy
    vfs/         ssg-vfs         mounts → union file view, walkers, ignore rules, PathParser
    pageparser/  ssg-pageparser  front matter split+decode, summary divider, shortcode lexer (lex + assemble)
    locale/      ssg-locale      ICU collation, plural rules, i18n bundles + message evaluator, numbers, localized dates
    page/        ssg-page        per-page rules: meta, dates, build policy, cascade, target paths/permalinks, sort, titles
    site/        ssg-site        capture + assembly → Model: trees, kinds, taxonomies, translations, relations, resources, data, get_page/ref
    nav/         ssg-nav         menus, pagination arithmetic + pager URLs, related index, alias plan
    markup/      ssg-markup      comrak + the Go implementation's passes: anchors, TOC/fragments, summary, word count, Hooks trait, context spans
    highlight/   ssg-highlight   syntect + two-face, Chroma class map, inline styles, fence options, CSS    [heavy]
    minify/      ssg-minify      minify-html / lightningcss / oxc / JSON / XML                           [heavy]
    images/      ssg-images      ImageSpec, ops, filters, codecs, deferred queue, cache                  [heavy]
    jsbuild/     ssg-jsbuild     js.Build in process with rolldown (2026-10-02; T14 built it as the esbuild --service client crate)
    resources/   ssg-resources   ResourceStore, bundles/assets/remote, transforms, pipes, URL-token resolution
    layouts/     ssg-layouts     template scan (v0.146 names), the Go lookup scorer, baseof variants, escaping, Tera loading
      embedded/                      the Go implementation's embedded templates rewritten in Tera (include_str!)
    funcs/       ssg-funcs       spec::FUNCS (source of truth) + pure Tera filters/functions/tests + tera-contrib subset
    view/        ssg-view        view structs, ViewCache, render-state types (RenderScope, PageStores, PaginationRecorder, DeferredRegistry), ContentRenderer trait
    sitefuncs/   ssg-sitefuncs   site-bound Tera functions as handle structs (get_page, ref, i18n, assets, images, paginate, partial…)
    render/      ssg-render      Session: content phase (shortcodes, hooks), layout jobs, memo cells; implements ContentRenderer
    publish/     ssg-publish     sinks, canonify, minify dispatch, build stats, URL-token extraction, held outputs, static sync
    build/       ssg-build       orchestration: phases, language sub-waves, pager wave, deferred wave, resource/image publish, report
    cli/         fugo (bin)          clap CLI: build, templates check, config, version; binary fugo
    serve/       ssg-serve       (T71) axum + livereload + notify, memory sink
    testkit/     ssg-testkit     (dev) plain-JSON fixture reader, txtar sites, insta settings, contract test
    migrate/     ssg-migrate     (COULD, T73) Go template → Tera converter, legacy file renames, printf/where translation
    workspace-hack/                  (internal) union features of light shared deps; every member depends on it
  sites/<site>/                      testsite | <private site> | docs
    layouts/**                       Tera overlay, v0.146 names, lower-case
    assets/**                        only assets used with execute_as_template (Tera)
    patches/{i01,reduced}/**         docs only: variant files mirroring sites.py layout patches 1:1
  testdata/
    oracle/<area>/…      engine-neutral fixtures moved in T00, converted to plain JSON (this project's schema)
    corpus/…             private-site md bodies + front matters, collation strings, minifier corpus, date corpus
    site-assets/…        jpg/png inputs sites.py needs
    golden/<site>/       Go manifests (L1–L4) + structure-oracle dumps; golden/images/ (20 Go-processed images for PSNR)
    baselines/<site>.json  ratchet baselines
tools/dev/
  oracle.sh  compare.sh  node.sh     native Go build; Go vs Rust per site (two passes, HTTP disabled); pinned npm ci
  node/package.json  node/package-lock.json
  manifest.py  structdiff.py  selftest.py  fixtures2json.py   Python stdlib only
  licence-check.sh  disk.sh
  changes/<task-id>.md               ratchet triage entries
  bin/  node_modules/                (gitignored)
tools/go-oracle/structure/           new Go program (T01)
tools/rust-port/i01/sites.py         kept; gains --overlay <dir> and --docs-patches i01|reduced, emits patches.json
```

The binary keeps its `-rs` name, so the harness scripts keep their names.

### 2.2 Cargo configuration (driven by the disk budget, §8.1)

```toml
# rust/Cargo.toml (excerpt)
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
license = "Apache-2.0"
rust-version = "1.94"          # local rustc 1.94.1; resolver 3 picks MSRV-compatible versions

[profile.dev]
debug = "line-tables-only"
incremental = false            # incremental caches are the largest consumer of target/

[profile.dev.package."*"]
debug = false
opt-level = 1

[profile.dev.package.image]
opt-level = 3
[profile.dev.package.fast_image_resize]
opt-level = 3
[profile.dev.package.zune-jpeg]
opt-level = 3
[profile.dev.package.libwebp-sys]        # the C build reads this crate's opt-level, not webp's
opt-level = 3
[profile.dev.package.syntect]
opt-level = 3
[profile.dev.package.fancy-regex]
opt-level = 3
[profile.dev.package.grass]
opt-level = 3
```

**Member crate rules.**
- `[lib] doctest = false`.
- `autotests = false` with a single `[[test]] name = "it" path = "tests/it/main.rs"`.
- `view`, `sitefuncs`, `render`, `build` and `cli` also set `[lib] test = false`. Their tests live only in `tests/it`, so they have one test binary each; each of these binaries links the heavy graph.
- No third-party `features =` in member manifests.
- Every member depends on `ssg-workspace-hack`.

**Feature unification.** Cargo unifies features per invocation, over the selected packages only. So `-p a` and `-p b` would otherwise build light shared dependencies (serde, regex, memchr, indexmap, hashbrown, smallvec, …) with different feature sets, giving duplicate artifacts.
- T00 writes `crates/workspace-hack` by hand. It depends on those light shared dependencies with the union of their features, checked with `cargo tree -e features -i <dep>`.
- Heavy crates never go into the hack.
- If cargo 1.94.1 has a stable `resolver.feature-unification = "workspace"` (T00 verifies), T00 uses it instead and drops the hack.

**Platform.** `cargo fetch` and `cargo metadata` always run with `--target x86_64-unknown-linux-gnu` / `--filter-platform x86_64-unknown-linux-gnu`. T00 commits `Cargo.lock`.

### 2.3 Crate dependency graph

Arrows point to dependencies. Every crate also depends on `base`.

```
base  (ids, PageKind, KindSet, Value/Map/Params, path newtypes, diag, Sink)
config      ── base
vfs         ── config
pageparser  ── base            locale ── base          esbuild ── base
minify*     ── config          images* ── config       highlight* ── config, markup(trait only)
page        ── config, vfs
site        ── page, pageparser, vfs, locale
nav         ── site
markup      ── config                         (defines Highlighter + Hooks traits)
layouts     ── config, vfs, tera
funcs       ── locale, tera, tera-contrib
resources   ── vfs, images, minify, esbuild, tera
view        ── site, nav, resources, markup, layouts(TemplateName), tera
sitefuncs   ── view, funcs, resources, locale
render      ── view, sitefuncs, layouts, funcs, markup, pageparser
publish     ── config, minify
build       ── render, publish, resources, images, highlight, esbuild
cli ── build, serve        serve ── build (+ its config, vfs, site, publish)        testkit (dev) ── base, funcs(spec), tera
```

- **Shared types live in `base`.** `PageKind`, `KindSet`, the ids and `trait Sink` are defined there. Neither `config` nor `layouts` depends on `page`.
- **URL tokens.** `ResourceStore` never depends on `publish`: `publish` produces plain `UrlTokens`, and `build` passes them to the store.
- **Images.** `view` reaches images only through `ResourceStore`.
- **Heavy crates** (marked `*`) never compile in the core lane (base → config → vfs → page → site → nav).
- **T00 proves the graph is acyclic** by writing stub crates with their real dependency edges and running `cargo metadata`.

### 2.4 Core types

The signatures below are illustrative. Names, ownership and crate boundaries are binding; field sets may still grow.

```rust
// ───────────── ssg-base ─────────────
pub mod id {
    pub trait Idx: Copy + Ord + std::hash::Hash { fn index(self) -> usize; fn from_index(i: usize) -> Self; }
    // defined by one macro: PageId(u32), LangIdx(u8), FormatId(u8), MediaTypeId(u16), ResourceId(u32),
    // TaxonomyIdx(u8), TermIdx(u32), ImageOpId(u64), FrameId(u64), TxnId(u64)
    pub struct IdVec<I: Idx, T> { /* Vec<T> + PhantomData<I> */ }   // Index<I>, iter_enumerated(), push() -> I
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageKind { Home, Section, Page, Taxonomy, Term, #[serde(rename = "404")] NotFound,
                    Sitemap, SitemapIndex, #[serde(rename = "robotstxt")] RobotsTxt }
pub struct KindSet(u16);

/// Front matter, config, data, params. Cheap to clone (Arc).
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Value { #[default] Null, Bool(bool), Int(i64), Float(f64), String(Arc<str>), Date(Date),
                 Array(Arc<Vec<Value>>), Map(Arc<Map>) }
/// Case-PRESERVING, byte-ordered map (Go ranges maps in sorted key order; upper case sorts first).
/// Data files, unmarshal results and template dicts keep their keys (`Name`, `baseURL`).
#[derive(Clone, Debug, Default, PartialEq)] pub struct Map(BTreeMap<Arc<str>, Value>);
/// Front matter / config / language params. Built only by `Params::fold`, which lower-cases keys
/// recursively EXCEPT inside arrays (Go's PrepareParams). Lookup is case-insensitive.
#[derive(Clone, Debug, Default, PartialEq)] pub struct Params(Map);
impl Params {
    pub fn fold(m: Map) -> Self;
    pub fn get(&self, key: &str) -> Option<&Value>;
    pub fn get_path(&self, dotted: &str) -> Option<&Value>;
    pub fn fill_missing_from(&mut self, other: &Params);   // cascade, language fallback
    pub fn merge_deep(&mut self, over: &Params);           // `params:` sub-map
}
/// TOML local dates stay Local until page::DateResolver applies the language time zone.
#[derive(Clone, Debug, PartialEq)] pub enum Date { Zoned(jiff::Zoned), Local(jiff::civil::DateTime) }
impl Value {
    pub fn from_toml(v: toml::Value) -> Self;             // toml::Datetime → Date (decode via toml::Table, no private keys)
    pub fn from_yaml(v: serde_saphyr::Value) -> Self;     // timestamps stay strings (Go)
    pub fn from_json(v: serde_json::Value) -> Self;
    pub fn to_tera(&self) -> tera::Value;                 // keeps key case and byte order
}
pub struct Clock(pub jiff::Timestamp);                     // --clock
pub fn parse_date(s: &str, tz: &jiff::tz::TimeZone) -> Result<jiff::Zoned, DateError>; // Go's accepted layouts as jiff strptime patterns

pub mod paths {                       // each newtype has ONE constructor; transforms cannot be mixed up
    pub struct ContentKey(Arc<str>);  // tree key == .Path: lower-case, ' '→'-', nothing else; "" = home
    impl ContentKey { pub fn from_source(rel: &str) -> Self; pub fn segments(&self) -> impl Iterator<Item = &str>;
                      pub fn parent(&self) -> Option<ContentKey>; pub fn first_segment(&self) -> &str;
                      pub fn starts_with_segments(&self, prefix: &ContentKey) -> bool; }
    pub struct TermKey(Arc<str>);     // lower("/"+plural+"/"+value), ' '→'-', NOT sanitised
    pub struct OutputPath(Arc<str>);  // sanitised file path under publishDir: "/posts/one/index.html"
    pub struct UrlPath(Arc<str>);     // unescaped link path "/posts/one/"; .escaped(): RFC 3986 segment set, upper-case hex
    pub struct Permalink(Arc<str>);   // absolute, escaped
    /// Keeps Unicode general categories L*, Nd, M* plus Go's punctuation allowlist (salvaged list);
    /// categories via `unicode-properties` (verify), not Go tables. Keeps Thai.
    pub fn sanitize_segment(s: &str) -> String;
    pub fn urlize(s: &str) -> String;
    pub fn clean(p: &str) -> String;
}
pub mod anchor { pub enum Style { Github, GithubAscii, Blackfriday }
                 pub fn anchorize(s: &str, st: Style) -> String; pub struct Deduper(/* -1, -2 suffixes */); }
pub mod inflect { pub fn pluralize(w: &str) -> String; pub fn singularize(w: &str) -> String;
                  pub fn humanize(s: &str) -> String; pub fn ordinalize(n: i64) -> String; }
pub mod title   { pub enum Style { Ap, Chicago, Go, FirstUpper, None } pub fn title_case(s: &str, st: Style) -> String; }
pub mod glob    { pub fn compile(pattern: &str, o: GlobOpts) -> Result<globset::GlobMatcher, GlobError>; } // literal_separator, {a,b}
pub trait Collate: Send + Sync { fn compare(&self, a: &str, b: &str) -> std::cmp::Ordering; }
pub mod diag {
    pub struct Position { pub file: Arc<Path>, pub line: u32, pub col: u32 }
    pub enum Severity { Error, Warning, Info }
    pub struct Diagnostic { pub severity: Severity, pub id: Option<String>, pub message: String,
                            pub position: Option<Position>, pub notes: Vec<String> }
    pub struct Diagnostics { /* Mutex<Vec<Diagnostic>>; dedupe by id|message; honours ignoreLogs */ }
}
pub trait Sink: Send + Sync {
    fn write(&self, path: &paths::OutputPath, bytes: &[u8]) -> std::io::Result<()>;
    fn exists(&self, path: &paths::OutputPath) -> bool;
}
```

```rust
// ───────────── ssg-config ─────────────
/// Typed CLI layer; serialises into the "flags" layer of the pipeline (§3.1 A1).
#[derive(Serialize, Default)] pub struct CliOverrides { pub base_url: Option<String>, pub environment: Option<String>,
    pub destination: Option<PathBuf>, pub minify: Option<bool>, pub build_drafts: Option<bool>,
    pub build_future: Option<bool>, pub build_expired: Option<bool> }
pub struct LoadOptions { pub source: PathBuf, pub config_files: Vec<PathBuf>, pub cli: CliOverrides,
                         pub env: Vec<(String, String)> /* the Go program's environment variables */ }
pub fn load(o: &LoadOptions) -> Result<Config, ConfigError>;   // errors carry toml/saphyr spans

pub struct Config {
    pub project_dir: PathBuf, pub environment: String,
    pub sites: IdVec<LangIdx, SiteConfig>,      // enabled languages sorted (weight, key); [0] = default
    pub output_formats: Arc<OutputFormats>, pub media_types: Arc<MediaTypes>,
    pub mounts: Vec<MountConfig>, pub themes: Vec<Theme> /* precedence order: dir, ThemeMounts */,
    pub build: BuildConfig /* buildStats, cachebusters */,
    pub caches: CachesConfig /* dirs with :cacheDir, :project resolved; maxAge */,
    pub security: SecurityPolicy, pub privacy: PrivacyConfig, pub imaging: ImagingConfig, pub minify: MinifyConfig,
    pub raw: Params,
}
pub enum Direction { Ltr, Rtl }
pub struct Language { pub idx: LangIdx, pub key: String, pub name: String, pub code: String, pub direction: Direction,
                      pub weight: i32, pub time_zone: jiff::tz::TimeZone, pub url_prefix: String /* "" or "th" */ }
pub struct SiteConfig {
    pub lang: LangIdx, pub language: Language, pub base_url: BaseUrl, pub title: String, pub copyright: String,
    pub params: Params, pub taxonomies: IdVec<TaxonomyIdx, TaxonomyDef>, pub outputs: KindOutputs,
    pub permalinks: Permalinks, pub pagination: PaginationConfig /* pager_size 10, path "page", aliases */,
    pub markup: MarkupConfig, pub front_matter: Vec<(DateField, Vec<DateSource>)>, pub related: RelatedConfig,
    pub sitemap: SitemapConfig, pub services: Services, pub menus: Vec<MenuEntryConfig>, pub cascade: Vec<CascadeConfig>,
    pub urls: UrlPolicy, pub disable_kinds: KindSet, pub aliases: AliasPolicy /* Write | Disabled */,
    pub robots_txt: RobotsPolicy, pub emoji: EmojiPolicy, pub titles: TitleConfig, pub summary_length: usize,
    pub content_dir: Option<PathBuf>, pub ref_links: RefLinksConfig,
}
pub enum DateSource { Field(String), Filename, FileModTime, Git }
pub struct TaxonomyDef { pub singular: String, pub plural: String }
pub enum UglyPolicy { Inherit, Always, Never }
pub enum Escaping { Html, Plain }
pub enum LinkPolicy { Own, UsePrimary }          // Go `permalinkable`
pub enum Listing { Alternative, NotAlternative }
pub enum Placement { LanguageDir, Root }          // Go `root`
pub struct OutputFormat { pub name: String, pub media_type: MediaTypeId, pub base_name: String, pub path: String,
    pub rel: String, pub protocol: String, pub escaping: Escaping, pub is_html: bool, pub ugly: UglyPolicy,
    pub links: LinkPolicy, pub listing: Listing, pub placement: Placement, pub weight: i32 }
pub struct OutputFormats { /* IdVec<FormatId, OutputFormat>, sorted (weight>0 first, then name) = render order */ }
pub struct MediaType { pub main: String, pub sub: String, pub suffixes: Vec<String>, pub delimiter: String }
```

```rust
// ───────────── ssg-vfs ─────────────
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Component { Content, Layouts, Assets, Data, I18n, Static, Archetypes }
pub struct Vfs { /* per component: ordered mounts (project, then themes); first match wins */ }
pub struct FileRef { pub component: Component, pub rel: String /* unix, original case */, pub abs: PathBuf,
                     pub mount_lang: Option<LangIdx>, pub mount_idx: u16 }
impl Vfs { pub fn new(cfg: &Config) -> Result<Self, VfsError>;
           pub fn walk(&self, c: Component) -> impl Iterator<Item = FileRef> + '_;   // byte order, ignore rules applied
           pub fn open(&self, c: Component, rel: &str) -> Option<FileRef>; }
pub enum BundleKind { Leaf, Branch, Single, ContentResource, Resource }
pub struct PathInfo { pub key: ContentKey, pub original: String, pub name: String, pub ext: String,
                      pub lang: Option<LangIdx>, pub format: Option<FormatId>, pub kind: BundleKind, pub section: String }
pub enum Parsed { File(PathInfo), DisabledLanguage }
pub struct PathParser<'a> { /* enabled + disabled language keys, output formats */ }
impl PathParser<'_> { pub fn parse(&self, c: Component, rel: &str) -> Parsed; }
```

```rust
// ───────────── ssg-pageparser ─────────────
pub enum FrontMatterFormat { Yaml, Toml, Json /* Org: COULD */ }
pub struct Split<'a> { pub front_matter: Option<(FrontMatterFormat, &'a str)>, pub body: &'a str, pub body_offset: usize }
pub fn split_front_matter(src: &str) -> Result<Split<'_>, ParseError>;        // BOM, CRLF, leading blank lines
pub fn decode_front_matter(f: FrontMatterFormat, text: &str) -> Result<Params, DecodeError>; // Params::fold
/// Flat token stream, checked against the 141,869-item oracle.
pub fn lex(body: &str) -> Result<Vec<Token<'_>>, ParseError>;
/// Go's IsInner changes parsing: a shortcode whose template uses `inner` must be closed.
pub enum InnerUse { Required, Unused, UnknownShortcode }
pub trait InnerOracle { fn inner_use(&self, shortcode: &str) -> InnerUse; }
pub fn assemble<'a>(tokens: Vec<Token<'a>>, oracle: &dyn InnerOracle) -> Result<Body<'a>, ParseError>;
pub struct Body<'a> { pub segments: Vec<Segment<'a>>, pub summary_divider: Option<usize> }
pub enum Segment<'a> { Text(&'a str), Shortcode(ShortcodeCall<'a>), Escaped(&'a str) /* {{</* */>}} */ }
pub enum Delim { Html /* {{< >}} */, Markdown /* {{% %}} */ }
pub enum Closing { SelfClosed, Closed(Vec<Segment<'a>>), Open }
pub struct ShortcodeCall<'a> { pub name: &'a str, pub delim: Delim, pub args: ShortcodeArgs, pub closing: Closing,
                               pub position: Position, pub span: Range<usize> }
pub enum ShortcodeArgs { None, Positional(Vec<Scalar>), Named(Vec<(String, Scalar)>) }
pub enum Scalar { String(String), Int(i64), Float(f64), Bool(bool) }   // "0.125.0" stays a String
```

```rust
// ───────────── ssg-locale ─────────────
pub struct Locale { /* icu_collator + icu_plurals per language */ }   // impl base::Collate
/// A message is parsed once into pieces. Only `{{ . }}`, `{{ .Field }}` (incl. `.Count`) and the
/// `{{-`/`-}}` trim markers are accepted; anything else is a load error with file and key.
pub enum Piece { Text(String), Dot, Field(Vec<String>) }
pub struct Translations { /* per language: key → plural category → Vec<Piece>; TOML/YAML/JSON, flat|nested, v1 id/translation */ }
impl Translations {
    pub fn load(vfs: &Vfs, langs: &IdVec<LangIdx, Language>) -> Result<Self, I18nError>;
    /// Fallback chain: lang → default language; enableMissingTranslationPlaceholders honoured.
    pub fn translate(&self, lang: LangIdx, key: &str, count: Option<f64>, data: &Value) -> Option<String>;
}
pub fn format_number(n: f64, precision: u8, lang: &Language) -> String;
pub enum DatePattern<'a> { Strftime(&'a str), Style(DateStyle /* Short|Medium|Long|Full */) }
pub fn format_date(d: &jiff::Zoned, p: DatePattern, lang: &Language) -> String;  // th → th-u-ca-gregory
```

```rust
// ───────────── ssg-page ─────────────
#[derive(Clone, Copy, Default)] pub enum ListMode { #[default] Always, Never, Local }
#[derive(Clone, Copy, Default)] pub enum RenderMode { #[default] Always, Never, Link }
#[derive(Clone, Copy)] pub struct BuildPolicy { pub list: ListMode, pub render: RenderMode, pub publish_resources: bool }
pub enum Markup { Markdown, Html }          // from front matter `markup`, else the content file extension
#[derive(Clone, Debug, Default)] pub struct Dates { pub date: Option<jiff::Zoned>, pub lastmod: Option<jiff::Zoned>,
                                                    pub publish_date: Option<jiff::Zoned>, pub expiry_date: Option<jiff::Zoned> }
/// Overrides applied at capture, before tree insertion (Go front matter `kind`, `lang`, `path`).
pub struct CaptureOverrides { pub kind: Option<PageKind>, pub lang: Option<String> /* lower-cased */, pub path: Option<String> }
pub struct PageMeta {
    pub title: Option<String>, pub link_title: Option<String>, pub description: String, pub summary: Option<String>,
    pub slug: Option<String>, pub url: Option<String>, pub r#type: Option<String>, pub layout: Option<String>,
    pub weight: i32, pub draft: bool, pub keywords: Vec<String>, pub aliases: Vec<String>,
    pub outputs: Option<Vec<FormatId>>, pub sitemap: SitemapConfig, pub build: BuildPolicy /* `headless` folded in */,
    pub translation_key: Option<String>, pub markup: Markup, pub cjk: bool,
    pub resources: Vec<ResourceMetaRule>, pub menus: Vec<PageMenuEntry>, pub cascade: Vec<CascadeRule>,
    pub dates: Dates, pub params: Params,      // reserved keys ALSO stay in params (R: where Params.type)
}
pub fn capture_overrides(p: &Params) -> Result<CaptureOverrides, PageError>;
pub fn meta_from_params(params: Params, ctx: &MetaCtx) -> Result<PageMeta, PageError>; // `params:` merged last

pub struct DateResolver { /* per-field Vec<DateSource>, alias expansion, :filename, :fileModTime, :git (COULD) */ }
impl DateResolver { pub fn resolve(&self, p: &mut Params, file: Option<&FileCtx>, tz: &jiff::tz::TimeZone) -> Result<Dates, PageError>; }

pub struct CascadeRule { pub target: CascadeTarget, pub values: Params }
pub struct CascadeTarget { pub path: Option<GlobMatcher>, pub kind: Option<GlobMatcher>,
                           pub lang: Option<GlobMatcher>, pub environment: Option<GlobMatcher> }
pub struct Cascade(Vec<CascadeRule>);
impl Cascade { pub fn inherit(parent: &Cascade, own: &Cascade) -> Cascade;
               pub fn apply(&self, m: &MatchCtx, p: &mut Params); }   // writes only absent keys

pub struct UrlInputs<'a> { /* kind, section key, container dir, base name, slug, url, permalink pattern,
                              dates, format, pager_segment, lang prefix, url policy, base url */ }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetPaths { pub target: OutputPath, pub resource_base: OutputPath, pub link: UrlPath }
pub struct Links { pub rel_permalink: UrlPath, pub permalink: Permalink }
pub fn target_paths(i: &UrlInputs) -> TargetPaths;                  // semantics §10.3–10.5
pub fn links(t: &TargetPaths, base: &BaseUrl, policy: &UrlPolicy) -> Links;
/// `:year :slug :sections[1:] :contentbasename …`; Go layout tokens (`:2006`) are translated to strftime at parse.
pub struct PermalinkPattern { /* … */ }
impl PermalinkPattern { pub fn parse(s: &str) -> Result<Self, PageError>; pub fn expand(&self, c: &PermalinkCtx) -> Result<String, PageError>; }

pub struct SortKey<'a> { pub weight: i32, pub date: Option<&'a jiff::Zoned>, pub link_title: &'a str,
                         pub key: &'a ContentKey, pub taxo: Option<(i32, u32)> }
pub fn default_order(a: &SortKey, b: &SortKey, c: &dyn Collate) -> std::cmp::Ordering; // weight(0 last)→date desc→title→path
pub fn default_title(kind: PageKind, raw: &str, cfg: &TitleConfig) -> String;
```

```rust
// ───────────── ssg-site ─────────────
pub enum PageRole { Standalone, BundledResource { owner: PageId } }   // bundled: rendered content, no output
pub struct Page {
    pub id: PageId, pub lang: LangIdx, pub kind: PageKind, pub role: PageRole,
    pub key: ContentKey,
    pub source: Option<SourceFile>,           // abs path, PathInfo, raw body Arc<str>, unique id; None for auto nodes
    pub meta: PageMeta,
    pub title: String, pub link_title: String, pub section: String, pub r#type: String,
    pub formats: Vec<FormatId>,               // [0] = primary
    pub urls: Vec<(FormatId, TargetPaths)>, pub links: Option<Links>,   // None if render != always or bundled
    pub parent: Option<PageId>, pub ancestors: Vec<PageId>, pub current_section: PageId, pub first_section: PageId,
    pub pages: Vec<PageId>, pub regular_pages: Vec<PageId>, pub sections: Vec<PageId>,
    pub nav: Neighbours, pub translations: Vec<PageId> /* incl. self, AllTranslations order */,
    pub terms: Vec<(TaxonomyIdx, TermIdx)>,   // front-matter order
    pub resources: Vec<ResourceId>,           // shared across translations (duplicateResourceFiles=false)
}
pub struct SiteModel { pub lang: LangIdx, pub tree: SiteTree, pub home: PageId,
                       pub pages: Vec<PageId>, pub regular_pages: Vec<PageId>,
                       pub taxonomies: IdVec<TaxonomyIdx, Taxonomy>, pub standalone: Standalone,
                       pub last_mod: Option<jiff::Zoned> }
pub struct SiteTree { by_key: BTreeMap<ContentKey, PageId> }      // byte order == Go walk order
impl SiteTree {
    pub fn get(&self, key: &ContentKey) -> Option<PageId>;
    /// SEGMENT-aware longest prefix + language Dir() retry (not go-radix character level; confirmed by T23b oracle).
    pub fn longest_prefix(&self, key: &ContentKey) -> Option<PageId>;
    pub fn descendants(&self, key: &ContentKey) -> impl Iterator<Item = PageId> + '_;
}
pub struct Taxonomy { pub def: TaxonomyDef, pub page: PageId, pub terms: IdVec<TermIdx, Term> /* sorted by key */ }
pub struct Term { pub key: TermKey, pub name: String /* first value */, pub term: String /* last value */,
                  pub page: PageId, pub members: Vec<WeightedPage> }
pub struct WeightedPage { pub page: PageId, pub weight: i32, pub ordinal: u32 }
/// Registered per bundle directory with the owning language; every translation of the owner shares it.
pub struct BundleResource { pub rel_name: String, pub abs: PathBuf, pub owner_dir: ContentKey, pub owner_lang: LangIdx,
                            pub target_base: OutputPath, pub meta: ResourceMeta, pub page: Option<PageId>,
                            pub publish: bool }
pub struct Model { pub config: Arc<Config>, pub pages: IdVec<PageId, Page>, pub sites: IdVec<LangIdx, SiteModel>,
                   pub bundle_resources: IdVec<ResourceId, BundleResource>, pub data: Arc<Map> /* case preserved */ }
impl Model {
    pub fn page(&self, id: PageId) -> &Page;
    pub fn get_page(&self, lang: LangIdx, reference: &str, from: Option<PageId>) -> Result<Option<PageId>, RefError>; // §19
    pub fn ref_link(&self, lang: LangIdx, r: &RefArgs, from: Option<PageId>, abs: bool) -> Result<String, RefError>;
}
pub mod data {
    /// data/**/*.{yaml,yml,toml,json,csv,xml}: directories nest, file stem is the key, deep merge
    /// project over themes, keys never folded (`/` inside a key is kept).
    pub fn load(vfs: &Vfs) -> Result<Map, DataError>;
}
pub struct LoadModelOptions { pub clock: Clock, pub drafts: bool, pub future: bool, pub expired: bool }
pub fn load_model(cfg: Arc<Config>, vfs: &Vfs, collate: &IdVec<LangIdx, Arc<dyn Collate>>, o: &LoadModelOptions)
    -> Result<Model, ModelError>;
```

```rust
// ───────────── ssg-nav ─────────────
pub struct Menus(pub IdVec<LangIdx, BTreeMap<String, Vec<MenuEntry>>>);
pub struct MenuEntry { pub identifier: String, pub name: String, pub title: String, pub url: String,
                       pub page: Option<PageId>, pub weight: i32, pub parent: Option<String>, pub pre: String,
                       pub post: String, pub params: Params, pub children: Vec<MenuEntry> }
pub fn build_menus(m: &Model) -> Result<Menus, NavError>;
impl Menus { pub fn is_current(&self, p: PageId, e: &MenuEntry) -> bool; pub fn has_current(&self, m: &Model, p: PageId, e: &MenuEntry) -> bool; }
pub struct Pagination { pub items: Arc<[PageId]>, pub size: usize, pub base: TargetPaths }
pub struct Pager { pub number: u32, pub range: Range<usize>, pub target: TargetPaths }
impl Pagination { pub fn pagers(&self, cfg: &PaginationConfig) -> Vec<Pager>; }   // always ≥ 1
pub fn default_pagination_list(m: &Model, p: PageId) -> Arc<[PageId]>;            // semantics §15.2
pub struct RelatedIndex { /* inverted index over [related] indices */ }
impl RelatedIndex { pub fn build(m: &Model, cfg: &RelatedConfig) -> Self;
                    /// Candidates are given explicitly (site.pages vs site.regular_pages differ).
                    pub fn related(&self, m: &Model, page: PageId, candidates: &[PageId], q: &RelatedQuery) -> Vec<PageId>; }
pub struct AliasPlan { pub from: OutputPath, pub to: PageId, pub format: FormatId }
pub fn alias_plan(m: &Model) -> Vec<AliasPlan>;
```

```rust
// ───────────── ssg-markup ─────────────
bitflags! { pub struct Extensions: u32 { TABLES; FOOTNOTES; DEFINITION_LISTS; DEFINITION_TERM_IDS; STRIKETHROUGH;
    TASKLISTS; LINKIFY; HEADING_ATTRIBUTES; BLOCK_ATTRIBUTES; ALERTS; EMOJI; } }
pub enum RawHtml { Omit, Pass }                 // goldmark `unsafe`
pub enum CodeFences { Hooked, Plain }           // Go markup.highlight.codeFences: Plain = no hook, no highlighter
pub struct TocOptions { pub start: u8, pub end: u8, pub ordered: bool }
pub struct MarkdownOptions { pub extensions: Extensions, pub raw_html: RawHtml, pub typographer: Option<Typographer>,
                             pub heading_ids: anchor::Style, pub passthrough: Vec<Delimiters>,
                             pub code_fences: CodeFences, pub toc: TocOptions }
pub struct Heading { pub id: String, pub level: u8, pub html: String, pub plain: String, pub children: Vec<Heading> }
pub struct Fragments { pub headings: Vec<Heading>, pub identifiers: Vec<String> }
impl Fragments { pub fn contains(&self, id: &str) -> bool; pub fn count(&self, id: &str) -> usize; }
pub struct Toc { pub headings: Vec<Heading> }
impl Toc { pub fn to_html(&self, o: &TocOptions) -> String; }

/// Byte ranges of the expanded source that came from another page (render_shortcodes includes).
pub struct SourceContexts(pub Vec<(Range<usize>, PageId)>);
pub struct ExpandedMarkdown<'a> { pub text: &'a str, pub page: PageId, pub contexts: &'a SourceContexts }
/// `inner_page` = innermost context span containing the node's source position (Go `.PageInner`).
pub struct HookEnv { pub page: PageId, pub inner_page: PageId, pub ordinal: u32, pub position: Position }
pub enum HookOut { Default, Html(String) }
/// Hooks run post-order on the comrak AST: inner hooks first, then the node is replaced by raw HTML.
pub trait Hooks: Sync {
    fn link(&self, _: &HookEnv, _: &LinkCtx) -> Result<HookOut, HookError> { Ok(HookOut::Default) }
    fn image(&self, _: &HookEnv, _: &ImageCtx) -> Result<HookOut, HookError> { Ok(HookOut::Default) }
    fn heading(&self, _: &HookEnv, _: &HeadingCtx) -> Result<HookOut, HookError> { Ok(HookOut::Default) }
    fn code_block(&self, _: &HookEnv, _: &CodeBlockCtx) -> Result<HookOut, HookError> { Ok(HookOut::Default) }
    fn blockquote(&self, _: &HookEnv, _: &BlockquoteCtx) -> Result<HookOut, HookError> { Ok(HookOut::Default) }
    fn table(&self, _: &HookEnv, _: &TableCtx) -> Result<HookOut, HookError> { Ok(HookOut::Default) }
    fn passthrough(&self, _: &HookEnv, _: &PassthroughCtx) -> Result<HookOut, HookError> { Ok(HookOut::Default) }
}
#[derive(Serialize)] pub struct LinkCtx { pub destination: String, pub title: String, pub text: String /* html */,
                                          pub plain_text: String, pub is_block: bool, pub attributes: Map }
pub type ImageCtx = LinkCtx;
#[derive(Serialize)] pub struct HeadingCtx { pub level: u8, pub anchor: String, pub text: String, pub plain_text: String, pub attributes: Map }
#[derive(Serialize)] pub struct CodeBlockCtx { #[serde(rename = "type")] pub lang: String, pub inner: String, pub options: Map, pub attributes: Map }
#[derive(Serialize)] #[serde(rename_all = "lowercase")] pub enum BlockquoteKind { Regular, Alert }
#[derive(Serialize)] #[serde(rename_all = "lowercase")] pub enum AlertSign { None, Plus, Minus }
#[derive(Serialize)] pub struct BlockquoteCtx { #[serde(rename = "type")] pub kind: BlockquoteKind, pub alert_type: String,
                                                pub alert_title: String, pub alert_sign: AlertSign, pub text: String, pub attributes: Map }
#[derive(Serialize)] #[serde(rename_all = "lowercase")] pub enum Alignment { None, Left, Center, Right }
#[derive(Serialize)] pub struct Cell { pub text: String, pub alignment: Alignment }
#[derive(Serialize)] pub struct TableCtx { pub thead: Vec<Vec<Cell>>, pub tbody: Vec<Vec<Cell>>, pub attributes: Map }
#[derive(Serialize)] #[serde(rename_all = "lowercase")] pub enum PassthroughKind { Inline, Block }
#[derive(Serialize)] pub struct PassthroughCtx { #[serde(rename = "type")] pub kind: PassthroughKind, pub inner: String, pub attributes: Map }
pub trait Highlighter: Send + Sync { fn highlight(&self, code: &str, lang: &str, o: &HighlightOptions) -> Result<String, HookError>; }

pub fn fragments(src: &ExpandedMarkdown, o: &MarkdownOptions) -> Fragments;      // parse-only, no hooks
pub fn render(src: &ExpandedMarkdown, o: &MarkdownOptions, h: &dyn Hooks, hl: Option<&dyn Highlighter>)
    -> Result<RenderedMarkdown, MarkupError>;
pub struct RenderedMarkdown { pub html: String, pub toc: Toc, pub fragments: Fragments }
pub struct Summary { pub html: String, pub truncated: bool }
pub mod text { pub fn strip_html(html: &str) -> String; pub fn word_count(plain: &str, cjk: bool) -> usize;
               pub fn auto_summary(html: &str, words: usize, cjk: bool) -> Summary;
               pub fn split_at_marker(html: &str, marker: &str) -> Option<(String, String)>; }
```

```rust
// ───────────── ssg-resources / images / esbuild ─────────────
pub enum ResourceKind { Image, Page(PageId), Text, Other }
pub enum PublishPolicy { Eager /* bundle, publishResources */, OnReference, Never }
pub struct Resource {
    pub id: ResourceId, pub kind: ResourceKind, pub media_type: MediaTypeId, pub name: String, pub title: String,
    pub params: Params, pub target: OutputPath, pub rel_permalink: UrlPath, pub permalink: Permalink,
    pub body: Body, pub image: Option<ImageMeta>, pub integrity: Option<String>, pub policy: PublishPolicy,
}
pub enum Body { File(PathBuf), Bytes(Arc<[u8]>), Generated(Arc<[u8]>) /* e.g. the stats file from E4 */,
                Derived { from: ResourceId, op: Transform }, PendingImage(ImageOpId),
                PostProcess(PostProcessId) /* one placeholder per field */ }
pub enum Transform { Fingerprint(HashAlgo), Minify, ToCss(SassOptions), PostCss(PostCssOptions), Tailwind(TailwindOptions),
                     Babel(BabelOptions), JsBuild(JsBuildOptions), Image(ImageOp), PostProcess }
/// Identity: transforms memoize on (source, chain); resources that NAME a target path (concat, from_string,
/// execute_as_template) memoize on that path. Within one language sub-wave, a second call with different
/// inputs for the same target is an error naming both call sites; across languages the earlier language wins.
pub struct ResourceStore { /* RwLock<IdVec<ResourceId, Arc<Resource>>>, key→id memo, target→id memo, url→id map */ }
impl ResourceStore {
    pub fn register_bundle(&self, r: &BundleResource) -> ResourceId;
    pub fn get_asset(&self, path: &str) -> Result<Option<ResourceId>, ResourceError>;
    pub fn find_assets(&self, glob: &str) -> Result<Vec<ResourceId>, ResourceError>;
    pub fn from_string(&self, target: &str, content: &str, call: CallSite) -> Result<ResourceId, ResourceError>;
    pub fn concat(&self, target: &str, items: &[ResourceId], call: CallSite) -> Result<ResourceId, ResourceError>;
    pub fn from_template_output(&self, target: &str, output: String, call: CallSite) -> Result<ResourceId, ResourceError>;
    pub fn get_remote(&self, url: &str, o: &RemoteOptions) -> Result<ResourceId, RemoteError>; // ureq + [caches.getresource]
    pub fn inject_generated(&self, asset_path: &str, bytes: Arc<[u8]>);        // stats file freshness (E4)
    pub fn transform(&self, id: ResourceId, t: Transform, env: &TransformEnv) -> Result<ResourceId, ResourceError>;
    pub fn resource(&self, id: ResourceId) -> Arc<Resource>;
    pub fn content(&self, id: ResourceId) -> Result<Arc<[u8]>, ResourceError>;       // realizes the chain
    pub fn mark_published(&self, id: ResourceId);                                     // `publish` filter
    /// Resolves URL tokens (raw forms) against rel_permalink/permalink of every registered resource.
    pub fn publish(&self, tokens: &UrlTokens, sink: &dyn Sink, images: &ImageQueue) -> Result<PublishStats, ResourceError>;
}

pub struct ImageSpec { pub action: Action, pub width: Option<u32>, pub height: Option<u32>, pub format: Option<ImageFormat>,
                       pub quality: Option<u8>, pub hint: WebpHint, pub filter: Resample, pub anchor: Anchor,
                       pub rotate: Option<i32>, pub background: Option<[u8; 4]> }
impl std::str::FromStr for ImageSpec { /* "600x400 webp q75 Lanczos Center #fff r90" (docs passes these in content) */ }
/// Deserialized from template maps: `{"op": "overlay", "image": r, "x": 10, "y": 10}`.
#[derive(Deserialize)] #[serde(tag = "op", rename_all = "snake_case")]
pub enum ImageFilter { Brightness { percentage: f32 }, Contrast { percentage: f32 }, Gamma { gamma: f32 },
    GaussianBlur { sigma: f32 }, Grayscale, Hue { shift: f32 }, Invert, Colorize { hue: f32, saturation: f32, percentage: f32 },
    ColorBalance { r: f32, g: f32, b: f32 }, Saturation { percentage: f32 }, Sepia { percentage: f32 },
    Sigmoid { midpoint: f32, factor: f32 }, UnsharpMask { sigma: f32, amount: f32, threshold: f32 }, Pixelate { size: u32 },
    Opacity { opacity: f32 }, Padding(PaddingSpec), Overlay { image: ResourceArg, x: i32, y: i32 }, Mask { image: ResourceArg },
    AutoOrient, Process { spec: String }, Text(TextSpec) /* COULD */, Dither(DitherSpec) /* COULD */ }
pub struct ImageQueue { /* Mutex<BTreeMap<ImageOpId, ImageOp>>, metadata cache, cache dir from [caches.images] */ }
impl ImageQueue {
    /// Returns final name and dimensions immediately (metadata only); pixels are processed in E6.
    pub fn enqueue(&self, src: &Path, spec: Option<&ImageSpec>, filters: &[ImageFilter]) -> Result<Enqueued, ImageError>;
    pub fn process(&self, wanted: &BTreeSet<ImageOpId>, sink: &dyn Sink) -> Result<(), ImageError>; // rayon, outside renders
}
pub struct Enqueued { pub id: ImageOpId, pub file_name: String /* name_hu_<xxh3>.<ext> */, pub width: u32, pub height: u32 }

pub struct Service { /* esbuild --service child process, request router (kept from nh-esbuild) */ }
impl Service { pub fn start(binary: &Path) -> Result<Self, EsbuildError>;   // version from `esbuild --version`
               pub fn build(&self, req: BuildRequest, plugins: &dyn ResolvePlugins) -> Result<BuildResult, EsbuildError>; }
```

```rust
// ───────────── ssg-layouts ─────────────
pub enum StandaloneKind { NotFound, Sitemap, SitemapIndex, RobotsTxt, Alias }
pub enum HookKind { Link, Image, Heading, CodeBlock, Blockquote, Table, Passthrough }
/// What a layout file IS, resolved at scan time from v0.146 names; unknown identifiers are load errors.
pub enum TemplateRole {
    Layout { kind: Option<PageKind>, layout: Option<String> },
    Base { kind: Option<PageKind>, layout: Option<String> },
    Partial { name: String }, Shortcode { name: String },
    Hook { kind: HookKind, variant: Option<String> },
    Standalone(StandaloneKind),
}
pub enum Origin { User(PathBuf), Theme(u8, PathBuf), Embedded }
/// Tera template name; constructible only inside this crate (incl. synthesized "layout@@base" variants).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)] pub struct TemplateName(Arc<str>);
pub struct TemplateInfo { pub name: TemplateName, pub role: TemplateRole, pub scope: ContentKey,
                          pub lang: Option<LangIdx>, pub format: Option<FormatId>, pub media: Option<MediaTypeId>,
                          pub escaping: Escaping, pub origin: Origin }
#[derive(PartialEq, Eq, PartialOrd, Ord)] pub struct Score { /* Go weights as ordered fields */ }
pub struct LayoutQuery<'a> { pub path: &'a ContentKey /* first segment replaced by `type` */, pub kind: PageKind,
                             pub layout: Option<&'a str>, pub lang: LangIdx, pub default_lang: bool, pub format: FormatId }
pub struct Selection { pub layout: TemplateName, pub base: Option<TemplateName>, pub render_as: TemplateName }
pub struct LayoutStore { templates: Vec<TemplateInfo> }
impl LayoutStore {
    /// v0.146 names only; `_default/`, `partials/`, `shortcodes/`, `taxonomy/list`, `term/term`, `X-baseof`
    /// are errors with a rename hint. Names are lower-cased.
    pub fn scan(vfs: &Vfs, cfg: &Config) -> Result<Self, TemplateError>;
    pub fn select(&self, q: &LayoutQuery) -> Option<Selection>;        // Go scorer + baseof
}
pub struct Templates { tera: tera::Tera, store: Arc<LayoutStore>, uses_inner: HashSet<TemplateName>, /* lookup memos */ }
/// Fallback prefixes, then `register` (all names), then ONE add_raw_templates call.
pub fn load(store: Arc<LayoutStore>, sel: &Selections, register: &dyn Fn(&mut tera::Tera)) -> Result<Templates, TemplateError>;
impl Templates {
    pub fn tera(&self) -> &tera::Tera;
    pub fn uses_variable(&self, t: &TemplateName, var: &str) -> bool;   // after load; follows includes + parents
    pub fn shortcode(&self, name: &str, scope: &ContentKey, fmt: FormatId, lang: LangIdx) -> Option<TemplateName>;
    pub fn hook(&self, h: HookKind, variant: Option<&str>, scope: &ContentKey, fmt: FormatId, lang: LangIdx,
                embedded: EmbeddedHooks) -> Option<TemplateName>;
}

// ───────────── ssg-funcs ─────────────
pub enum NameKind { Filter, Function, Test }
pub enum PhaseAvail { Content, Layout, Both }
pub struct Kwarg { pub name: &'static str, pub ty: &'static str, pub required: bool }
pub struct FuncSpec { pub name: &'static str, pub kind: NameKind, pub kwargs: &'static [Kwarg], pub phase: PhaseAvail,
                      pub safe: bool, pub site_bound: bool, pub go: &'static str, pub doc: &'static str }
pub mod spec { pub const FUNCS: &[FuncSpec] = &[/* source of truth; template-api.md is generated from it */];
               pub const EMBEDDED_TEMPLATES: &[&str] = &[/* names T32 provides */]; }
pub struct FuncsEnv { pub collators: IdVec<LangIdx, Arc<dyn Collate>>, pub security: SecurityPolicy, pub clock: Clock }
pub fn register_pure(t: &mut tera::Tera, env: Arc<FuncsEnv>);    // incl. tera-contrib subset
pub fn register_placeholders(t: &mut tera::Tera);                // every FUNCS entry (contract test, templates check)
/// Typed handles passed through Tera values; errors read "expected a resource, got a page".
#[derive(Deserialize)] pub struct ResourceArg { #[serde(rename = "__rid")] pub rid: u32 }
#[derive(Deserialize)] pub struct PageArg { pub id: u32, pub kind: PageKind }
pub fn arg<T: DeserializeOwned>(v: &tera::Value, what: &str) -> tera::TeraResult<T>;
```

### 2.5 Views and render state (ssg-view)

**Serialisation.** Views derive `Serialize` and are converted once with `tera::Value::from_serializable`. Nested `tera::Value` fields pass through as `Arc` clones.

**Every documented key is always present**, with `none` for absent values, because Tera 2 raises an error when an undefined value is printed. A test renders a template that prints every documented key, for every kind.

```rust
#[derive(Serialize)] pub struct DateView { pub rfc3339: String, pub unix: i64 }   // compare instants via .unix
/// Relation-free page value. One per page per generation; Arc-shared in every list.
#[derive(Serialize)]
pub struct PageSummaryView {
    pub id: u32, pub kind: PageKind, pub lang: String, pub path: String, pub section: String,
    pub r#type: String, pub layout: Option<String>, pub bundle_type: Option<BundleType>,
    pub title: String, pub link_title: String, pub description: String,
    pub date: Option<DateView>, pub lastmod: Option<DateView>, pub publish_date: Option<DateView>, pub expiry_date: Option<DateView>,
    pub weight: i32, pub draft: bool, pub params: tera::Value, pub keywords: Vec<String>, pub aliases: Vec<String>,
    pub permalink: String, pub rel_permalink: String,          // primary format (semantics §10.6); "" when not linkable
    pub is_home: bool, pub is_section: bool, pub is_page: bool, pub is_node: bool, pub is_translated: bool,
    pub file: Option<FileView>, pub git_info: Option<GitInfoView> /* none unless :git (COULD) */,
    pub sitemap: SitemapView, pub language: LanguageView,
    pub output_formats: tera::Value,                            // {name: OutputFormatView}, format order
    pub resources: tera::Value,                                 // [ResourceView]
    pub terms: tera::Value,                                     // {plural: [PageLink]} — a key for EVERY configured taxonomy
    pub raw_content: String,                                    // source after the front matter: known after parsing, so in
                                                                // every generation (F8: shortcodes read other pages' sources)
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub content: Option<ContentView>,                           // Full generations only
}
#[derive(Serialize)] pub struct FileView { pub path: String, pub dir: String, pub base_file_name: String,
                                           pub content_base_name: String, pub unique_id: String, pub is_content_adapter: bool }
#[derive(Serialize)] pub struct SitemapView { pub change_freq: String, pub priority: f64, pub disable: bool }
#[derive(Serialize)] pub struct ContentView {
    pub content: tera::Value /* safe */, pub summary: tera::Value /* safe */, pub truncated: bool,
    pub plain: String, pub word_count: usize, pub fuzzy_word_count: usize,
    pub reading_time: usize, pub table_of_contents: tera::Value /* safe */, pub fragments: FragmentsView, pub len: usize,
}
#[derive(Serialize)] pub struct FragmentsView { pub headings: Vec<HeadingView>, pub identifiers: Vec<String> }
#[derive(Serialize)] pub struct HeadingView { pub id: String, pub level: u8, pub title: String, pub headings: Vec<HeadingView> }
/// Inserted on top of a summary map to form the full value (the rendered page, `deref`, `get_page`).
/// Every list holds SUMMARY values → acyclic, Arc-shared.
pub struct PageRelations {
    pub parent: Option<tera::Value>, pub current_section: tera::Value, pub first_section: tera::Value,
    pub ancestors: tera::Value, pub pages: tera::Value, pub regular_pages: tera::Value,
    pub regular_pages_recursive: tera::Value, pub sections: tera::Value,
    pub prev: Option<tera::Value>, pub next: Option<tera::Value>,
    pub prev_in_section: Option<tera::Value>, pub next_in_section: Option<tera::Value>,
    pub translations: tera::Value, pub all_translations: tera::Value,
    pub alternative_output_formats: tera::Value,
    pub taxonomy: Option<TaxonomyView>,   // taxonomy pages
    pub term: Option<TermView>,           // term pages
}
#[derive(Serialize)] pub struct TaxonomyView { pub singular: String, pub plural: String,
                                               pub terms: tera::Value /* [TermEntryView], alphabetical */ }
#[derive(Serialize)] pub struct TermView { pub name: String, pub term: String, pub key: String, pub singular: String, pub plural: String }
#[derive(Serialize)] pub struct TermEntryView { pub name: String, pub key: String, pub count: usize,
                                                pub page: tera::Value /* summary */, pub pages: tera::Value }
#[derive(Serialize)] pub struct PageLink { pub id: u32, pub kind: PageKind, pub path: String, pub lang: String, pub title: String,
                                           pub link_title: String, pub permalink: String, pub rel_permalink: String }
#[derive(Serialize)] pub struct SiteView {
    pub title: String, pub base_url: String, pub lang: String, pub language_code: String, pub language: LanguageView,
    pub languages: Vec<LanguageView>, pub is_multilingual: bool, pub copyright: String, pub params: tera::Value,
    pub data: tera::Value /* shared, case-preserved */, pub home: tera::Value, pub pages: tera::Value,
    pub regular_pages: tera::Value, pub all_pages: tera::Value, pub sections: tera::Value,
    pub taxonomies: tera::Value /* {plural: {term_key: TermEntryView}} sorted */,
    pub menus: tera::Value /* {menu: [MenuEntryView]} */, pub last_mod: Option<DateView>,
    pub config: SiteConfigView /* services.rss.limit, services.google_analytics.id, privacy.* */,
    pub sitemap_abs_url: Option<String>,
}
#[derive(Serialize)] pub struct LanguageView { pub lang: String, pub name: String, pub code: String, pub direction: String, pub weight: i32, pub params: tera::Value }
#[derive(Serialize)] pub struct MenuEntryView { pub identifier: String, pub name: String, pub title: String, pub url: String,
                                                pub weight: i32, pub pre: String, pub post: String, pub params: tera::Value,
                                                pub page: Option<PageLink>, pub children: Vec<MenuEntryView>, pub has_children: bool }
#[derive(Serialize)] pub struct MediaTypeView { pub r#type: String /* "text/html"; use for {{ .MediaType }} */,
                                                pub main_type: String, pub sub_type: String, pub suffixes: Vec<String>, pub delimiter: String }
#[derive(Serialize)] pub struct OutputFormatView { pub name: String, pub rel: String, pub media_type: MediaTypeView,
                                                   pub permalink: String, pub rel_permalink: String, pub is_plain_text: bool, pub is_html: bool }
#[derive(Serialize)] pub struct PagerView { pub page_number: u32, pub url: String, pub pages: tera::Value, pub pager_size: usize,
                                            pub total_pages: u32, pub total_number_of_elements: usize, pub has_prev: bool,
                                            pub has_next: bool, pub prev: Option<PagerLink>, pub next: Option<PagerLink>,
                                            pub first: PagerLink, pub last: PagerLink, pub pagers: Vec<PagerLink> }
#[derive(Serialize)] pub struct PagerLink { pub page_number: u32, pub url: String }
#[derive(Serialize)] pub struct ResourceView { #[serde(rename = "__rid")] pub rid: u32, pub name: String, pub title: String,
                                               pub params: tera::Value, pub resource_type: ResourceType, pub media_type: MediaTypeView,
                                               pub rel_permalink: String, pub permalink: String,   // per-field placeholders for post_process
                                               pub width: Option<u32>, pub height: Option<u32>, pub data: ResourceDataView /* integrity */,
                                               pub page_id: Option<u32> /* content resources: resource_content → page HTML */ }
#[derive(Serialize)] pub struct ShortcodeView { pub name: String, pub args: Vec<tera::Value>, pub params: tera::Value,
                                                pub is_named_params: bool, pub ordinal: u32,
                                                pub parent: Option<Box<ShortcodeView>>, pub position: String }
#[derive(Serialize)] pub struct BuildView { pub version: &'static str /* "0.149.0-DEV" */, pub app_version: &'static str,
                                           pub environment: String, pub is_production: bool, pub is_development: bool,
                                           pub is_server: bool, pub generator: tera::Value /* safe */ }
```

```rust
// ── render state (defined in view; implemented/owned by render) ──
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)] pub enum Phase { Content, Layout, Deferred }
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HookVariant { Html, Format(FormatId) }   // Format only if some `_markup/*.<fmt>.*` hook exists
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)] pub enum Stage { Expand, Fragments, Content(HookVariant) }
/// Serialized into every render context under SCOPE_KEY and read back by site functions. No thread-locals.
#[derive(Clone, Serialize, Deserialize)]
pub struct RenderScope { pub page: PageId, pub lang: LangIdx, pub format: FormatId, pub pager: Option<u32>,
                         pub phase: Phase, pub variant: HookVariant, pub frame: Option<FrameId>, pub txn: Option<TxnId>,
                         pub depth: u16 /* partial/render nesting, limit 64 */, pub chain: Vec<(PageId, Stage)> }
pub const SCOPE_KEY: &str = "__nh";    // T02 verifies `@__nh` is a valid implicit parameter, else renamed `nh_scope`
impl RenderScope { pub fn from_state(s: &tera::State) -> tera::TeraResult<Option<Self>>; pub fn child(&self) -> Self; }

/// The only callback from site functions into the render engine.
pub trait ContentRenderer: Send + Sync {
    fn content(&self, p: PageId, v: HookVariant, s: &RenderScope) -> Result<Arc<RenderedContent>, ContentError>;
    fn fragments(&self, p: PageId, s: &RenderScope) -> Result<Arc<Fragments>, ContentError>;
    fn render_shortcodes(&self, p: PageId, s: &RenderScope) -> Result<Arc<ExpandedSource>, ContentError>;
    fn render_markdown(&self, md: &str, o: RenderStringOptions, s: &RenderScope) -> Result<String, ContentError>;
    fn render_template(&self, t: &TemplateName, ctx: tera::Context, s: &RenderScope) -> Result<String, ContentError>;
}
pub struct ExpandedSource { pub markdown: String, pub placeholders: Vec<Arc<str>> /* page-local NHSC<n>X */,
                            pub contexts: SourceContexts }
/// Page stores: layout-phase writes apply directly; content-phase writes are buffered per TxnId and
/// committed only by the computation that wins its memo cell (duplicate work stays side-effect-free).
pub struct PageStores { /* IdVec<PageId, RwLock<Map>>, DashMap<TxnId, Vec<(PageId, String, Value)>> */ }
pub struct PaginationRecorder { /* Mutex<BTreeMap<(PageId, FormatId), Recorded { pagination: Arc<Pagination>, first_call: Position }>> */ }
pub struct DeferredRegistry { /* Mutex<BTreeMap<String /*key*/, Deferred { template, data }>> */ }

pub struct ViewGeneration {
    pub summaries: IdVec<PageId, tera::Value>, pub links: IdVec<PageId, tera::Value>,
    full: IdVec<PageId, OnceLock<tera::Value>>,   // pure view assembly, no rendering → blocking init is safe
    pub sites: IdVec<LangIdx, tera::Value>,
}
/// Meta generation for the content phase; Full generations (one per hook variant) frozen in phase D.
pub struct ViewCache { meta: ViewGeneration, full: OnceLock<BTreeMap<HookVariant, ViewGeneration>> }
impl ViewCache { pub fn generation(&self, phase: Phase, v: HookVariant) -> &ViewGeneration; }
```

### 2.6 Render pipeline API

```rust
// ───────────── ssg-sitefuncs ─────────────
/// Everything site functions need; each function struct clones only its own Arcs.
pub struct Handles { pub model: Arc<Model>, pub views: Arc<ViewCache>, pub store: Arc<ResourceStore>, pub images: Arc<ImageQueue>,
                     pub stores: Arc<PageStores>, pub pagination: Arc<PaginationRecorder>, pub deferred: Arc<DeferredRegistry>,
                     pub menus: Arc<Menus>, pub related: Arc<RelatedIndex>, pub i18n: Arc<Translations>,
                     pub diagnostics: Arc<Diagnostics>, pub renderer: Arc<OnceLock<Weak<dyn ContentRenderer>>>,
                     pub frames: Arc<DashMap<FrameId, Value>>, pub partial_cache: Arc<DashMap<(String, String), PartialResult>> }
pub fn register(t: &mut tera::Tera, h: &Handles);

// ───────────── ssg-render ─────────────
pub struct Session { /* Arc<Model>, Arc<Templates>, Arc<ViewCache>, ContentStore, Handles, render_pool, Diagnostics */ }
impl Session {
    /// renderer slot created empty → sitefuncs::register → layouts::load (fallible) → Arc::new(Session) → slot.set(weak)
    pub fn new(project: Project, model: Arc<Model>, o: &RenderOptions) -> Result<Arc<Self>, RenderError>;
    pub fn render_content(&self) -> Result<(), RenderError>;       // phase C1, all variants
    pub fn freeze_views(&self) -> Result<(), RenderError>;         // phase D
    pub fn render_job(&self, job: &Job) -> Result<Vec<Output>, RenderError>;  // phase E, pure: no I/O
}
pub struct ContentStore {
    expanded: IdVec<PageId, Memo<Arc<ExpandedSource>>>,
    frags: IdVec<PageId, Memo<Arc<Fragments>>>,
    content: BTreeMap<HookVariant, IdVec<PageId, Memo<Arc<RenderedContent>>>>,  // variants known after template load
}
/// Never blocks: set → return; key in scope.chain → cycle error naming both positions; else compute with
/// chain+key and `set` (the loser discards its result and its buffered store writes).
pub struct Memo<T>(OnceLock<T>);
impl<T: Clone> Memo<T> { pub fn get_or_compute<E>(&self, key: (PageId, Stage), scope: &RenderScope,
                                                  f: impl FnOnce(&RenderScope) -> Result<T, E>) -> Result<T, E>; }
pub enum Job {
    Alias(AliasPlan),
    Page { page: PageId, format: FormatId },                   // pager 1 when paginated
    Pager { page: PageId, format: FormatId, number: u32 },     // wave 2, number ≥ 2
    PagerAlias { page: PageId, format: FormatId },             // …/page/1/ → node (HTML formats only)
    Standalone { page: PageId, format: FormatId },             // 404, sitemap, sitemapindex, robots
    LanguageRedirect,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)] pub struct JobOrder { lang: LangIdx, format_rank: u8, key_rank: u32, sub: u8 }

// ───────────── ssg-publish ─────────────
pub struct DiskSink { pub root: PathBuf }                      // impl base::Sink
pub struct MemorySink { pub files: DashMap<OutputPath, Arc<[u8]>> }
pub struct Output { pub path: OutputPath, pub bytes: Vec<u8>, pub media: MediaTypeId, pub format: FormatId,
                    pub is_html: bool, pub order: JobOrder }
/// URL-shaped tokens from template outputs, after decoding HTML entities (&amp; &#39; &#x27;) and JSON escapes
/// (\u0026 \/), split on whitespace/quotes/parens/commas (srcset), with and without ?query/#fragment.
pub struct UrlTokens(BTreeSet<String>);
pub struct Publisher { /* sink, canonify rewriter, minifier, StatsCollector (per-thread sets), UrlTokens, held outputs */ }
impl Publisher {
    pub fn emit(&self, o: Output) -> Result<(), PublishError>;  // canonify → stats → tokens → hold if placeholders | minify → write
    pub fn add_tokens_from(&self, text: &str);                  // execute_as_template outputs
    pub fn stats(&self) -> StatsFile;
    pub fn patch_held(&self, repl: &BTreeMap<String, String>) -> Result<(), PublishError>; // re-extract tokens, minify, write
    pub fn url_tokens(&self) -> UrlTokens;
}
pub fn sync_static(vfs: &Vfs, sink: &dyn Sink) -> Result<usize, PublishError>;

// ───────────── ssg-build ─────────────
pub struct BuildRequest { pub source: PathBuf, pub destination: Option<PathBuf>, pub cli: CliOverrides,
                          pub clock: Option<jiff::Timestamp>, pub sink: SinkKind, pub clean_destination: bool }
pub fn build(r: BuildRequest) -> Result<BuildReport, BuildError>;
#[derive(thiserror::Error, Debug)]
pub enum BuildError { #[error(transparent)] Config(#[from] ConfigError), #[error(transparent)] Model(#[from] ModelError),
                      #[error(transparent)] Template(#[from] TemplateError), #[error(transparent)] Render(#[from] RenderError),
                      #[error(transparent)] Publish(#[from] PublishError), #[error(transparent)] Resource(#[from] ResourceError),
                      #[error("{} errors", .0.len())] Diagnostics(Vec<Diagnostic>) }
pub struct BuildReport { pub pages: usize, pub outputs: usize, pub aliases: usize, pub resources: usize,
                         pub images: usize, pub collisions: Vec<Collision>, pub diagnostics: Vec<Diagnostic>,
                         pub timings: Vec<(&'static str, Duration)>, pub memory: Option<Arc<MemorySink>> }
```

---

## 3. Build data flow

### 3.1 Phases

| # | Phase | Crate | Input → output | Parallelism |
|---|---|---|---|---|
| A1 | **Config** | config | See the pipeline below | sequential |
| A2 | **Mounts** | vfs | Config → `Vfs`. Default mounts only for unconfigured components; `lang` on mounts; themes after the project, in `Config::themes` order (each theme's configured mounts, else its component directories). | sequential |
| A3 | **Discover** | vfs | Walk content mounts → `FileRef` + `PathInfo`. Ignore rules apply; files in a disabled language are dropped. | `par_iter` over mounts |
| A4 | **Parse** | pageparser, page, site, locale | Read, split and decode front matter → folded `Params` + body `Arc<str>`. `data::load` → case-preserved `Map`. `Translations::load` → messages parsed into pieces. | **`par_iter` over files** |
| B1 | **Assemble tree** | site | Apply `capture_overrides` (kind, lang, path) **before** insertion. Insert into the per-language `SiteTree` in key order. Bundle ownership is the segment-aware longest-prefix owner. A duplicate (Base, lang) keeps the first and warns. Bundled content files get `PageRole::BundledResource`. Bundle resources are registered once per bundle directory with the owning language. | sequential per language, languages in parallel |
| B2 | **Cascade → meta → dates → filter** | site, page | Tree-order walk: inherited cascade (own front matter wins), `meta_from_params`, `DateResolver` (language time zone), build policy (`headless` folded in), drafts/future/expired against `--clock`. A deleted page takes its subtree with it. | walk sequential; per-page decode parallel |
| B3 | **Auto nodes** | site | Root sections; taxonomy pages, even with no terms; term pages; home; standalone pages (404 and sitemap per site, sitemapindex and robots once). | sequential |
| B4 | **URLs** | page, site | kind, type, section, format → `TargetPaths` + `Links`, **after** cascade | **`par_iter` over pages** |
| B5 | **Lists and relations** | site | Default-sorted pages, regular pages and sections; next/prev; translations (sharing `ResourceId`s); node dates (max); taxonomies and weights | sorts `par_iter` over nodes |
| B6 | **Navigation** | nav | `Menus` (pageRef resolved), `RelatedIndex`, `Vec<AliasPlan>` | small |
| B7 | **Templates** | layouts | `LayoutStore::scan` (v0.146 names only); selection for every (page, format) and standalone page; baseof variants; `Session::new` loads Tera once, validating all names | selection `par_iter` |
| C0 | **Meta views** | view | Model → Meta generation (summaries without content; full values lazy) | `par_iter` |
| C1 | **Content** | render | For each page with a body, bundled content resources included, and each hook variant: (a) lex, assemble, execute shortcodes → `ExpandedSource` (page-local placeholders, context spans); (b) fragments, parse-only; (c) render: comrak for `Markup::Markdown`, pass-through for `Markup::Html`, hooks with `page_inner`, `CodeFences`; (d) swap placeholders; summary, plain text, word counts, TOC | **`par_iter` over pages**; memo cells for cross-page access |
| D | **Full views** | view | Build one Full generation per hook variant and freeze them in the `OnceLock`, all **before** E2 | `par_iter` |
| E1 | **Static copy** | publish | Static mounts → sink. Runs first, so rendered outputs win conflicts. | `par_iter` |
| E2 | **Wave 1** | build, render, publish | **One sub-wave per language, in language order.** Each sub-wave renders aliases, then pages × formats (pager 1), then standalone pages. Pagination calls are recorded. Every output goes through `Publisher::emit`. | `jobs.par_iter()` within a sub-wave |
| E3 | **Wave 2** | build | Pagers 2..N from the recorder, `page/1/` aliases (HTML formats only), language redirect | `par_iter` |
| E4 | **Stats** | publish, resources | Merge per-thread sets → the stats file (`build_stats.json`), sorted, honouring `disableIDs`. Write it to the project root (as the Go implementation does; external tools read it from disk) and `inject_generated` it for its mounted asset path. | reduce |
| E5 | **Deferred wave** | build, resources | Render each `defer(...)` template once per key (docs: Tailwind). Realise `post_process` fields (the private site's PostCSS purge reads the stats). This gives a replacement map → `patch_held`, which re-extracts URL tokens, then minify and write. | `par_iter` over deferred keys |
| E6 | **Publish resources and images** | resources, images | Eager bundle resources, plus every resource whose URL is in `UrlTokens`, plus explicit `publish`. Only referenced image ops are processed, into `[caches.images]`, then copied. No scan of plain CSS/JS text. | **`par_iter`** (outside any render) |
| E7 | **Report** | build, cli | Sorted, de-duplicated diagnostics; errors fail the build | – |

**A1 config pipeline** (rust-style; there is no mapstructure port):
1. **Bootstrap.** Read `environment`, `source` and `configDir` from `CliOverrides` and the environment variables.
2. **Load sources into `Value` trees:**
   - the project's config file: the first that exists of the project-named `.{toml,yaml,yml,json}` file, then the Go program's name and `config.*` (compatibility with the Go implementation's sites; today only `config.*` is read, HANDOFF.md §2); when several exist, a warning names the file read and the ones ignored. `--config a,b` lists the files explicitly;
   - `config/_default/**`, then `config/<env>/**`, with the file-name → key rules of semantics §1.2 (the project-named, the Go-program-named and `config.*` files there are root files).
3. **Normalise every tree.**
   - `normalize_keys`: lower-case keys except inside arrays; `menu`→`menus`; drop `internal`.
   - `migrate_legacy_keys`:
     - `indexes`→`taxonomies`
     - `paginate`→`pagination.pagerSize`
     - `paginatePath`→`pagination.path`
     - `rssLimit`→`services.rss.limit`
     - `pygments*`→`markup.highlight.*`
     - `writeStats`→`build.buildStats.enable`
     - `footnoteReturnLinkContents`→`markup.goldmark.extensions.footnote.backlinkHTML`
     - `ignoreErrors`→`ignoreLogs`
     - bool `minify`→`minify.minifyOutput`
4. **Deep-merge once**, in this precedence: defaults < file < dir < CLI < env.
   - Env is applied once. Each value is parsed into the variant of the value it overrides.
   - `disableKinds` and `disableLanguages` are split on commas and whitespace.
   - **Themes** below the project (the Go implementation's module collection and `_merge` semantics; `ssg-config` README "Themes"):
     - found in import order, depth first: `[[module.imports]]`, then `theme = [...]`, then each theme's own imports after it (`theme = ["a", "b"]` with `a` importing `c`: a, c, b; the first wins); in `themesDir`, `_vendor` (`modules.txt`) or at an absolute path; `module.replacements`, `ignoreConfig`, `ignoreImports`, `noMounts`, `disable`; modules are not downloaded;
     - each theme's config: the first of the project-named, the Go-program-named and `config.*` files in its directory, then its `config/_default/**` and `config/<env>/**`;
     - merged theme by theme: the project's values win; a theme adds only keys the project lacks, as the `_merge` strategy of the project's table allows (`params` deep, `menus`/`outputFormats`/`mediaTypes` shallow, other root keys none unless the root sets `_merge`; `languages.X.params` deep, `languages.X.menus` shallow; inherited below); root values that are not tables only with a `deep` root; a theme's `theme`, `module` and `themesDir` are never merged.
     - Deliberate deviations: no module download (a theme that is not found is always an error); a theme's `theme`/`module`/`themesDir`/bootstrap keys never merge (Go merges them under a `deep` root after using the project's); `_merge` values ignore case; a project non-table value where a theme has a table is kept (Go panics).
5. **Per-language merge.** For each enabled language, merge `languages.X` over the root tree.
6. **Typed deserialise.** serde into structs with `#[serde(default)]` and `impl Default` holding the Go implementation's defaults. `[frontmatter]` chains become `Vec<DateSource>` and `outputs` become `Vec<FormatId>`. Errors carry toml/saphyr spans.

### 3.2 Content phase

**Shortcodes run before markdown.**
- The lexer runs over the whole body, including code fences. That is why the docs site writes `{{</* */>}}`.
- `{{% %}}` output is spliced into the markdown source.
- `{{< >}}` output becomes a page-local ASCII token `NHSC<n-hex>X`, stored in the page's `ExpandedSource.placeholders`. It is swapped in after markdown, and a `<p>TOKEN</p>` wrapper is removed.

**`render_shortcodes(page=q)` inside `{{% include %}}`** returns q's `ExpandedSource`. The including page does three things:
- appends q's placeholders to its own table and renumbers the tokens in the spliced text;
- shifts q's context spans;
- adds a span `(range, q)` for the spliced text.

Markdown hooks inside that range receive `inner_page = q` (Go `.PageInner`). That is how the docs link hook resolves relative links and resources in `_common/*` snippets. The spans are computed from comrak `sourcepos`; T04 verifies accuracy for inline nodes. No textual markers ever reach the markdown, so `unsafe=false` raw-HTML omission is not affected.

**Inner content.**
- `InnerUse` is known statically from `Templates::uses_variable(tpl, "inner" | "inner_deindent")`, following includes and parents.
- The inner of the outermost `{{% %}}` is raw.
- A nested `{{% %}}` has its inner rendered as markdown. A single line loses its `<p>`.
- The v1 shortcode semantics (the in-template version declaration) are **not** reproduced (D5).
- `ordinal` counts per nesting level; `parent` is the enclosing call.

**Fragments are their own memo stage.** The docs link hook validates anchors in other pages, so A links to B and B links to A. The fragments stage parses the expanded source and never runs hooks.

**Code fences.**
- With `CodeFences::Plain` (the testsite, and docs under the I01 patches), fences render as `<pre><code class="language-x">` with no hook and no highlighter.
- With `Hooked`, the user or embedded code-block hook (including goat) runs. A fence with no hook goes to the `Highlighter`.

**HTML content** (`Markup::Html`, for example R `blog/html-page.html`) skips comrak. Shortcodes, the summary divider and the auto summary still apply.

**Hooks and the page store.** Hooks write the page store during C1 (docs `hasMath`, `hasToc`). The writes are buffered per `TxnId` and committed by the winning computation, so the store is complete before layouts run. The docs `{{ $noop := .WordCount }}` trick is deleted during conversion.

**Per-format content variants.** A page's content is rendered for every `HookVariant`. `Html` always exists. `Format(F)` exists only when some `_markup/*.<F>.*` template exists (the reconstruction's `render-table.json.json`). All variants are rendered in C1 and frozen in D. A layout job for format F uses variant `Format(F)` if it exists, else `Html`.

**Cross-page access** (`page_content`, `page_fragments`, `render_shortcodes`, `markdownify` with `page=`) goes through `Memo::get_or_compute` with the caller's `RenderScope`.
- A key already in `scope.chain` is a cycle error naming both positions.
- Two threads computing the same key both finish without blocking; the first `set` wins.

### 3.3 Layout jobs and pagination

**Context.** Each job gets `page` (the full value from the variant's generation), `site`, `build`, `lang`, `output_format` and `__nh` (§4.2).

**Pagination is recorded at call time** (semantics spec §15.1).
- `paginator()` uses the default list (semantics §15.2) and `pagination.pagerSize`. `paginate(pages=, size=?)` uses a custom list.
- The first call per (page, format) records the `Pagination` and its template position. Later calls behave as follows:

  | Later call | Result |
  |---|---|
  | `paginator()` | the stored pagination |
  | `paginate` with the same ids and size | the stored pagination |
  | `paginate` with a different list or size | error naming both positions |

  The private site's conversion drops the redundant second `.Paginate (sort …)`.
- Wave 2 renders pagers 2..N with `__nh.pager = N`. The same calls, including calls inside `partial()`, return pager N.
- A template that never calls either function produces no pager pages. This covers the private site's conditional paginator use and its paginated 404 page.

**Autoescape** follows the output format (§4.5).

**Ordering and collisions.**
- Jobs are ordered by `JobOrder` (language, format rank, tree key).
- Target collisions are resolved **before** rendering: a page beats an alias, and among pages the later `JobOrder` wins, with a warning.
- Go picks a nondeterministic winner for the private site's 13 colliding term directories. Ours is stable and listed in the ratchet as `accepted-deviation`.

### 3.4 Publishing, placeholders and the deferred wave

**Eager publishing.** Bundle resources of rendered pages whose `publishResources=true` (semantics §22.1) are published once per bundle directory, under the owning language's `target_base`. Translations share the file.

**Publishing on reference.** Assets, remote resources, transformed resources and `publishResources=false` bundles are published only when their URL appears in a template output.
- `Publisher::emit` extracts `UrlTokens` from every output, after decoding HTML entities and JSON escapes. Paths with `&` and `'` (S/R bundles `herrs-salt-&-vinegar…`, `Lay's`) therefore match.
- `execute_as_template` outputs are scanned too, because they are template output.
- Plain CSS and JS files are never scanned: the Go implementation publishes only through template calls.
- `publish` forces publication.
- Tokens are resolved in E6 against `url → ResourceId`, using both `rel_permalink` and `permalink`.

**Held outputs.**
- Outputs that contain `__nh_defer_<key>__` or `__nh_pp_<id>_<field>__` placeholders are held until E5 (written to the sink unpatched and patched there, so they take no memory); everything else is written immediately.
- A `post_process` resource view carries one placeholder per field: `content`, `rel_permalink`, `permalink`, `data.integrity`, `media_type`. `resource_content` returns the content placeholder.
- After patching, tokens are extracted again, so the PostProcess CSS URL is seen.
- Memory is bounded by the placeholder-bearing pages: about 50 MB for docs (all its HTML, because baseof has the deferred block), and a small set for the private site.

**Order inside `emit`:**
1. canonify / relative-URL rewrite;
2. collect stats (html5gum);
3. extract URL tokens;
4. hold the output, or minify and write it.

**External tools** (esbuild service, PostCSS, Tailwind, optional Dart Sass) run from rayon workers behind a semaphore of `min(4, cpus)`.

### 3.5 Determinism and where rayon is not used

**Sequential steps:**
- assembly walks B1–B3;
- config loading;
- the stats merge;
- work within one deferred key;
- the language order of E2 sub-waves. Target-path-memoised assets depend on it: the Go implementation renders languages in order, so the earlier language wins.

**Rules:**
- Every parallel stage collects into indexed or sorted collections before any order-dependent step.
- A render never starts parallel work. `debug_assert!(render_pool.current_thread_index().is_none())` guards every `par_*` call site in our crates. Images are processed in E6.
- The render pool uses `ThreadPoolBuilder::stack_size(16 << 20)`. Render nesting is capped by `__nh.depth ≤ 64`. Tera itself caps component recursion at 20.
- `DashMap` guards are never held while rendering. For example, `partial_cached` runs get → drop → compute → insert.

**Gate from T36 onwards:** the output tree hash is identical with `RAYON_NUM_THREADS=1` and `=8`, and across two runs at `=8`.

---

## 4. Template model

### 4.1 Engine setup

1. **Layout scan.** `LayoutStore::scan` reads the `Layouts` component of the Vfs: project first, then themes.
   - It accepts **only v0.146 names**: `_partials/`, `_shortcodes/`, `_markup/render-<kind>[-<variant>][.<fmt>].<ext>`, `baseof[.<kind>|.<layout>].html`, `home|section|taxonomy|term|single|list|all|<layout>[.<lang>][.<fmt>].<ext>`, `404.html`, `rss.xml`, `sitemap.xml`, `robots.txt`, `alias.html`, with directory path scopes.
   - Legacy names (`_default/`, `partials/`, `shortcodes/`, `taxonomy/list`, `term/term`, `X-baseof`) are load errors with a rename hint.
   - Names are lower-cased.
   - The legacy → new mapping exists only in the T01 structure-oracle normaliser and in `ssg-migrate`.
2. **One Tera instance per build**, built inside `Session::new`:
   - `tera.set_fallback_prefixes(["_theme1/", …, "_embedded/"])` is called **before** any template is added. User templates are unprefixed and win. The page scorer knows each template's origin and does not rely on the fallback.
   - `sitefuncs::register`, `funcs::register_pure` and the tera-contrib subset are registered before templates are added, because Tera 2 validates every name when a template is added. Site functions hold `Arc<OnceLock<Weak<dyn ContentRenderer>>>`; the slot is filled after the `Session` `Arc` exists.
   - A **single** `add_raw_templates` call adds user, theme and embedded templates plus the synthesised baseof variants. It validates syntax, names, inheritance and include cycles.
   - Go-template markers (`{{ .`, `{{ end }}`, `{{ define`) give an error that points to `fugo templates check` and the migrate tool.
3. **The contract instance** (T02, testkit, `templates check`) is identical, except that it calls `funcs::register_placeholders` and registers empty stubs for every `spec::EMBEDDED_TEMPLATES` name not yet written.
4. **Tera features:** `no_fmt`, `fast_hash`, `preserve_order`. Not `fast_escape`, which would stop escaping `'`. `glob_fs` is not used; all loading goes through the Vfs.

### 4.2 Contexts and the render scope

| Render | Top-level names |
|---|---|
| Layout job | `page` (full), `site`, `build`, `lang`, `output_format`, `__nh` |
| Shortcode | `page` (full value of the Meta generation: relations yes, content fields no), `site`, `build`, `lang`, `shortcode` (`ShortcodeView`), `inner` (safe), `inner_deindent`, `__nh` |
| Render hook | `page` and `page_inner` (Meta generation), `site`, `build`, `lang`, `__nh`, plus the hook fields **flattened** (below) |
| `partial(name=…, …)` | the kwargs as top-level names, plus the caller's `page`, `site`, `build`, `lang`, `output_format`, and a child `__nh` (same page, format and pager; new frame; depth+1) |
| Component | only its arguments. Implicit `@page`, `@site`, `@build`, `@lang` and `@__nh` may be declared (Tera looks each up by name in the caller's scope; `build` is in every render that can call a component). |
| `defer` template | `data`, `site`, `build`, `__nh` (phase `Deferred`) |
| `execute_as_template` | `data`, `site`, `build`, `__nh` |
| Alias | `permalink`, `page` (link), `site`, `build` |
| Sitemap, robots, 404 | `page` (the standalone page; its `pages` is `site.pages`), `site`, `build`, `lang`, `__nh` |
| Sitemapindex | the same, plus `sites: [{language, sitemap_abs_url, last_mod}]` |

Flattened hook fields, per hook:

| Hook | Fields |
|---|---|
| link, image | `destination`, `title`, `text`, `plain_text`, `is_block`, `attributes`, `ordinal`, `position` |
| heading | `level`, `anchor`, `text`, `plain_text`, `attributes` |
| codeblock | `type`, `inner`, `options`, `attributes`, `ordinal`, `position` |
| blockquote | `type`, `alert_type`, `alert_title`, `alert_sign`, `text`, `attributes`, `ordinal` |
| table | `thead`, `tbody`, `attributes`, `ordinal` |
| passthrough | `type`, `inner`, `attributes`, `ordinal`, `position` |

**How site functions get their scope.** Every site-bound function resolves it with `RenderScope::from_state`.
- Page-relative functions (`store_*`, `param`, `ref`, `rel_ref`, `get_page`, `i18n`, `rel_lang_url`, `related`, `is_menu_current`, `page_content`, …) also accept an explicit `page=` argument. Language and format then come from that page's primary format.
- Inside a component, where `__nh` is invisible unless declared with `@__nh`, a site-bound call without `page=` is an error with a hint.
- `paginator`, `paginate`, `return_value` and `defer` need the real scope, so a component must declare `@__nh` to use them.
- `templates check` lints both rules.

**Tera strictness.**
- Views always emit every documented field. Printing a missing param key is an error, so converted templates print `page.params.x or ""`.
- Nested lookups that may be missing use `?.`, for example `page.params.a?.b` or `page.parent?.title`, because a missing non-final segment is an error even inside `if`.

### 4.3 Layout lookup, baseof, partials, components, shortcodes, hooks, defer

**Lookup.** A pure, data-driven port of the Go implementation's scorer (semantics §11.4–11.6) using `TemplateRole` and `Score`.
- The first path segment is replaced by `type` when `type` differs from the section.
- Candidates come from walking from the root down to the query path.
- A user template beats a theme template, which beats an embedded one. Link and image hooks follow `useEmbedded`.
- Selections are computed eagerly in B7.
- No match gives a warning and no file; standalone pages are skipped silently.

**Baseof.**
- A layout that begins with `{% extends "baseof.html" %}` requests the Go implementation's base resolution.
- For every (layout L, resolved baseof B) pair where B is not the root `baseof.html`, a variant `TemplateName` `L@@B` is registered with its `extends` literal rewritten to B.
- The three target sites each have one root baseof, so no variants are created for them.
- An explicit `{% extends "other.html" %}` is left alone.
- A child `{% block %}` that its parent does not define is a Tera load error. Conversion deletes such blocks, for example the docs `home.html` `leftsidebar`.

**Partials** (conversion rule in §4.7):

| Go partial | Tera form |
|---|---|
| shares the caller's context (`partial "x" .`) | `{% include "_partials/x.html" %}` |
| literal name with dict arguments | a **component**, defined in `_partials/` files (`{% component breadcrumbs(page, sep="/", @lang) %}`) and called as `{{ <breadcrumbs page=page /> }}` |
| dynamic name, or returns a value | the function `partial(name=…, …)` |

- Component calls are validated at load time.
- `partial()` details:
  - It allocates a `FrameId` in `Handles.frames` and puts it into the child `__nh`.
  - **`return_value(value=expr)`** writes to that frame and prints nothing.
  - On exit, if the frame was set, `partial()` returns that value (a map, list or resource view). Otherwise it returns the rendered string, marked safe in HTML templates.
  - Nested value-returning partials work, because each call has its own frame id. This covers the docs partials `get-featured-image`, `get-page-images`, `get-github-info`, `get-remote-data`, `color-from-string` and `get-resource`.
- `partial_cached(name=, key=, …)` memoises the full `PartialResult` (string or value) on (name, key).
- `set_global` inside an include is discarded by Tera, so values flow out of partials only through `partial()` + `return_value`.

**Components** are global and can recurse, which suits docs `render-toc-level`/`docs-explorer-section` and the private site's `breadcrumbnav`. They are hygienic: see the scope rules in §4.2.

**Shortcodes.**
- `_shortcodes/<name>.html` (and `.md` / `.txt`) are ordinary Tera templates, looked up with path scope, output format and language.
- Positional arguments are `shortcode.args`, named arguments `shortcode.params`. `shortcode | arg(index=0, default="")` and `shortcode | arg(name="src", default="")` give Go's `.Get` with its missing-value rules.
- Content keeps the Go implementation's syntax exactly.
- Shortcodes cannot be components, because Go-template shortcodes take positional arguments.

**Render hooks.**
- `TeraHooks` implements `markup::Hooks`: it looks up `_markup/render-<kind>[-<variant>][.<format>].html` and renders it with `page_inner` from `HookEnv`.
- A hook for a non-default output format falls back to the HTML hook.
- Embedded link and image hooks apply when `useEmbedded` resolves to `fallback`, the default in multilingual single-host sites (T and S).
- The embedded table hook's default output is produced natively in Rust unless a user or theme hook exists.
- Under `CodeFences::Plain`, no code-block hook runs.

**`templates.Defer`** becomes `{{ defer(template="_partials/deferred/tailwind.html", key="global", data=…) }}`. It returns a placeholder; the template is rendered once per key in E5.

### 4.4 What each phase can see

| Data | Content phase (shortcodes, hooks, `markdownify`) | Layout phase |
|---|---|---|
| Metadata, URLs, params, resources, terms of any page | summary and full values of the Meta generation | summary and full values of the Full generation for the job's variant |
| Relations of a listed page | `p \| deref` or `get_page(path=p.path)` | the same |
| Raw source (`raw_content`, after the front matter) of any page | field of the Meta generation (known after parsing, before rendering) | the same |
| Content-derived data of this or another page | functions `page_content(page=)`, `page_summary`, `page_plain`, `page_word_count`, `page_fragments`, `page_toc`, `render_shortcodes(page=)`: lazy, memoised, cycle-checked through `__nh.chain` | fields `p.content`, `p.summary`, `p.plain`, `p.fragments`, … (the functions also work) |
| Content of a bundled content resource | `resource_content` on a view with `page_id` → that page's HTML (safe) | the same |
| Page store | `store_set(key=, value=)`, `store_get(key=)`; writes buffered per transaction | writes applied directly; visible later in the same render |
| Template-local state | `{% set %}`, `{% set_global %}`, `merge` | the same |

The docs `quick-reference` shortcode reads `.Content` of child sections, and becomes `page_content(page=s)`. Using a content field in the content phase produces Tera's undefined-field error, plus a sitefuncs note: `Field 'content' is not available in the content phase; use page_content(page=…)`.

### 4.5 Escaping

**Autoescape.** Tera's autoescape is set with `autoescape_on([".html", ".htm", ".xml", ".svg"])`.
- Output formats with `Escaping::Plain` are registered under plain-text alias names, so the format decides, not the file name. `home.redir`, `home.headers`, `home.json`, `index.json` and `robots.txt` are not escaped. HTML, `list.rss.xml` and `sitemap.xml` are escaped.
- An include inherits the caller's mode.

**Safe outputs.** Functions and filters that produce HTML are safe: `partial` (HTML mode), components, `markdownify`, `render_string`, `render_shortcodes`, `page_content`, `highlight`, `emojify`, `to_math`, `diagrams_goat`, `jsonify`, `html_escape`. Content, summary, TOC, hook `text` and `inner` are inserted as safe strings.

**`html_escape`** always escapes, even an input marked safe, and returns a safe value. The Go functions `html`, `htmlEscape` and `transform.HTMLEscape` map to it; R `rss.xml` `{{ .Summary | html }}` is the case that needs it. Tera's `escape` leaves safe values alone, so it is not used for these.

**No contextual escaping.**
- In `<script>`, use `jsonify | safe`. `jsonify` escapes `<`, `>` and `&` as `\u003c`, `\u003e` and `\u0026`.
- In query strings, use `urlencode`.
- `safeHTML`, `safeHTMLAttr`, `safeURL`, `safeJS` and `safeCSS` all become `safe`.
- `#ZgotmplZ` has no equivalent (D5).
- Escaped bytes differ from html/template's (T60): Tera writes `"` as `&quot;` and leaves `+`, Go writes `&#34;` and `&#43;`. The text is the same after entity decoding (L3); an embedded template that must match Go byte for byte avoids such characters or escapes itself.

### 4.6 Function, filter and test catalogue

Frozen as `ssg_funcs::spec::FUNCS` by T02. `template-api.md` is generated from it.

**Kind codes:**

| Code | Meaning |
|---|---|
| op | Tera operator or syntax |
| bi | Tera built-in |
| tc | tera-contrib |
| F | our filter |
| fn | our function |
| T | test |
| (s) | site-bound (ssg-sitefuncs); everything else is pure (ssg-funcs) |

**Naming.**
- Tera built-ins are never overridden.
- Names follow Tera/Zola verb_noun style.
- The Go implementation's concept names stay (`ref`, `rel_ref`, `abs_url`, `paginate`, `i18n`, `by_*`).

**Sites:** D = docs, S = the real private site, R = its reconstruction, T = testsite.

| Go | fugo Tera | Kind | Sites |
|---|---|---|---|
| `and` `or` `not` `eq` `ne` `lt` `le` `gt` `ge` | `and` `or` `not` `==` `!=` `<` `<=` `>` `>=`. Pages compare by `.id`, pagers by `.page_number`, dates by `.unix`. | op | all |
| `default X v` (bool is always set; 0, "", empty and none are unset) | `v \| default_if_empty(value=X)`; Tera `default(value=)` only for undefined values | F/bi | D S R |
| `cond c a b` | `a if c else b` | op | D R |
| `len` | `length` | bi | all |
| `print`, `printf` | `~`; `pad_start(width=)`, `pad_end(width=)` (`%-35s`); `round(precision=)` / `format_number(precision=)` (`%0.1f`); a literal U+00A0 character in the string (`%c` of 160; see the string note below the table); `'"' ~ x ~ '"'` under autoescape (`%q` in attributes), `jsonify` (`%q` in JS). No printf. | op/F | D S R |
| `errorf`, `warnf`, `erroridf`, `warnidf` | `log_error(message=)` (records an error; the build fails at the end), `log_warn(message=, id=?)`; `throw(message=)` aborts at once | fn/bi | D R |
| `dict`, `slice` | map and array literals; computed keys: `[[k, v]] \| from_pairs` (Go's `dict $path v` nesting: wrap once per key, innermost first) | op/F | all |
| `index m k`, `index m "a" "b"` | `m[k]`, `m.k`, `m \| get_path(path=["a","b"])` | op/F | all |
| `in`, `strings.Contains` | `x in l`, `"x" in s`; pages `p.id in [q.id for q in l]` | op | D |
| `isset`, `reflect.IsMap`, `reflect.IsSlice` | `"k" in m`, `is defined`, `is map`, `is array` | op/T | D S R |
| `first N`, `last N`, `after N` | `l[:N]`, `l[-N:]`, `l[N:]` | op | all |
| `append` | `l \| append(value=x)`, `l \| concat(with=other)` | F | D R |
| `merge` | `a \| merge(with=b)` (deep, right wins, sorted keys) | F | D R |
| `where` | `[p for p in pages if p.params.x == v]`; `in`/`intersect` via ids; `like` via `is matching(pat=)` | op/tc | D S R |
| `sort l "key" "desc"` | `sort_by(attribute=, reverse=?)`: collation-aware per `lang`, dates compared as instants | F (s) | D S R |
| `.ByTitle` `.ByDate` `.ByWeight` `.ByPublishDate` `.ByLastmod` `.ByLinkTitle` `.Reverse` | `by_title` `by_date` `by_weight` `by_publish_date` `by_lastmod` `by_link_title`; `reverse` (bi) | F (s)/bi | D |
| `.GroupByDate "2006"`, `GroupByParam` | `group_by_date(format="%Y", attribute=?)` → `[{key, pages}]`, `group_by_param(param=)` | F (s) | D |
| `complement` `union` `intersect` `symdiff` `uniq` | `complement(without=)` `union(with=)` `intersect(with=)` `symdiff(with=)` (pages by id), `unique` (bi) | F/bi | D |
| `delimit` | `join(sep=)`, `delimit(sep=, last=?)` | bi/F | D R |
| `seq N` | `range(start=1, end=N+1)` (end is exclusive) | bi | S R |
| `apply l "float" "."` | `[x \| float for x in l]` | op | D |
| `querify` | `querify(params={…})` (sorted) | fn | D |
| `newScratch`, `.Scratch.*` | `{% set %}`, `{% set_global %}`, `merge` (docs `datatable` and `root-configuration-keys` are rewritten this way) | op | D S R |
| `lower` `upper` | `lower` `upper` | bi | all |
| `title`, `strings.Title` | `title_case(style=?)` (`titleCaseStyle`, AP by default) | F | D S R |
| `trim s cutset`, `strings.TrimSpace` | `trim_chars(chars=)`, `trim` | F/bi | D S R |
| `strings.TrimLeft`/`TrimRight`/`TrimPrefix`/`TrimSuffix` | `trim_start_chars(chars=)` `trim_end_chars(chars=)` `strip_prefix(prefix=)` `strip_suffix(suffix=)` | F | D |
| `split`, `replace` | `split(pat=)`, `replace(from=, to=)` (T02 verifies kwarg names) | bi | D |
| `replaceRE`, `findRE` | `regex_replace(pattern=, rep=)`, `regex_find(pattern=, limit=?)` | tc/F | D |
| `substr` | `substr(start=, length=?)` (characters) | F | D |
| `truncate` (HTML-aware) | `truncate_html(length=, ellipsis=?)` | F | R |
| `strings.HasPrefix`/`HasSuffix` | `is starting_with(pat=)`, `is ending_with(pat=)` | T | embedded |
| `humanize`, ordinalize | `humanize`, `ordinalize` | F | D S R |
| `inflect.Pluralize`/`Singularize` | `pluralize_word`, `singularize_word` | F | D |
| `urlize`, `anchorize` | `urlize`, `anchorize(style=?)` (Go-exact) | F | D S R |
| `absURL` `relURL` `absLangURL` `relLangURL` | `abs_url` `rel_url` `abs_lang_url` `rel_lang_url` (base path, language prefix, canonify) | F (s) | all |
| `ref`, `relref` | `ref(path=, lang=?, output_format=?, page=?)`, `rel_ref(…)`; `refLinksErrorLevel` | fn (s) | D S R |
| `urls.Parse`, `urls.JoinPath` | `parse_url` → `{scheme, host, path, fragment, query, is_absolute, string}`; `join_url(parts=)` | F/fn | D R |
| `urlquery`, `urldecode` | `urlencode` / `urlencode_strict` (tc), `urldecode` | tc/F | S |
| `add sub mul div mod` | `+ - * / %`; `//` for Go's integer `div` | op | D S R |
| `math.Ceil/Floor/Round`, `math.Max/Min` | `round(method=)` (bi), `max(values=)`, `min(values=)` | bi/fn | D |
| `int` `float` `string` | `int` `float` `str` | bi | D |
| `safeHTML` `safeHTMLAttr` `safeURL` `safeJS` `safeCSS` | `safe` | bi | all |
| `html`, `htmlEscape`, `transform.HTMLEscape` | `html_escape` (always escapes; safe result) | F | S R D |
| `htmlUnescape`, `transform.XMLEscape` | `html_unescape`, `escape_xml` (bi) | F/bi | D |
| `jsonify (dict "indent" "  ")` | `jsonify(indent=?)` (sorted keys, HTML-safe, safe) | F | S R T |
| `transform.Unmarshal` | `unmarshal(format=?)` on a string or resource view (JSON/TOML/YAML/CSV/XML; keys preserved, sorted) | F (s) | D S R |
| `transform.Remarshal` | `remarshal(format="toml"\|"yaml"\|"json")` | F | D (607) |
| `markdownify`, `.RenderString (dict "display" "block")` | `markdownify`, `render_string(display=?, page=?)` (uses that page's hooks) | F (s) | D S R |
| `plainify`, `emojify` | `plainify` (html5gum), `emojify` | F | D R |
| `highlight`, `transform.Highlight` | `highlight(lang=, options=?)` (syntect; Chroma classes or inline styles per `noClasses`; `hl_inline`) | F | D |
| `transform.ToMath` (+ `try`) | `to_math(options=?, optional=?)` (KaTeX 0.16.22 with mhchem in QuickJS, rquickjs: the Go implementation's markup byte for byte; its defaults and `KatexOptions`; SHOULD, feature `math`). An error (a formula KaTeX rejects, invalid options) fails the render; with `optional=true` a warning (id `to_math`, as `get_remote`) and none | F | D |
| `diagrams.Goat` | `diagrams_goat(text=)` → `{inner (safe SVG), width, height, wrapped}` (svgbob; SHOULD, T66) | F | D |
| `base64Encode/Decode`, `md5`, `sha1`, `sha256`, `hash.FNV32a`, `hash.XxHash` | `base64_encode`/`base64_decode` (tc), `md5`, `sha1`, `sha256`, `fnv32a`, `xxhash` | tc/F | D |
| `now` | `now()` (honours `--clock`) | fn | all |
| `.Format`, `time.Format`, `dateFormat`, `:date_long` | `.Format` → `date(format="%B %-d, %Y")` (English names, like Go's `.Format`); `time.Format`/`dateFormat` → `date(format=…, locale=lang)` (localised names, F3); `date(style="long")` (localised; Thai uses the Gregorian calendar) | tc or F | D S R |
| `time.AsTime`, `.Year`, `.IsZero` | `to_date`, `d \| date(format="%Y")`, `is none` (zero dates serialise as `none`) | F/T | D |
| `i18n` / `T` | `i18n(key=, count=?, data=?, page=?)` | fn (s) | S R |
| `lang.FormatNumber` | `format_number(precision=)` | F (s) | D |
| `resources.Get` / `Match` / `GetMatch` | `get_asset(path=)`, `find_assets(pattern=)`, `find_asset(pattern=)` | fn (s) | D S R |
| `resources.GetRemote` (+ `try`) | `get_remote(url=, options={headers, method, body, key}?, optional=?)`. Errors propagate; with `optional=true` a failure returns `none` and logs a warning. | fn (s) | D S |
| `resources.Concat`, `resources.FromString` | `concat_assets(target=, items=[…])`, `asset_from_string(target=, content=)` (target-path identity) | fn (s) | S R |
| `.Resources.Get`/`GetMatch`/`Match`/`ByType` | `page.resources \| get_resource(name=)`, `find_resource(pattern=)`, `find_resources(pattern=)`, `by_type(type=)` (case-insensitive globs) | F | D S R |
| `fingerprint`, `minify`, `.Data.Integrity`, `.Content`, `.Publish` | `fingerprint(algo=?)`, `minify`, `r.data.integrity`, `resource_content`, `publish` | F (s) | D S R |
| `toCSS`/`css.Sass`, `postCSS`, `css.TailwindCSS`, `babel`, `js.Build`, `resources.ExecuteAsTemplate`, `resources.PostProcess` | `to_css(options=)`, `postcss(options=)`, `tailwind(options=)`, `babel(options=)`, `js_build(options=)`, `execute_as_template(target=, data=)` (the asset is a Tera template), `post_process` | F (s) | D S R |
| `.Resize` `.Fill` `.Fit` `.Crop` `.Process`, `.Width`, `.Height` | `resize(width=?, height=?, format=?, quality=?, filter=?, anchor=?, spec=?)`, likewise `fill` `fit` `crop` `process`; `r.width`, `r.height` (known immediately) | F (s) | D S R |
| `images.Filter`, `.Filter`, `images.*` constructors | `image_filter(filters=[{"op": "overlay", "image": logo, "x": 10, "y": 10}, {"op": "grayscale"}])` | F (s) | D S R |
| `images.Text`, `images.Dither`, `images.QR`, `.Exif`, `.Colors` | `{"op": "text", …}`, `{"op": "dither", …}` (T72a), `qr_code(text=, level=?, scale=?, target_dir=?)` (T72a: Go's bytes and name), `exif`, `image_colors` | F/fn (s) | D |
| `partial`, `partialCached`, `return`, `templates.Exists`, `templates.Defer` | include / component / `partial(name=, …)`, `partial_cached(name=, key=, …)`, `return_value(value=)`, `template_exists(name=)`, `defer(template=, key=, data=?)` | op/fn (s) | all |
| `site.GetPage`, `.GetPage` | `get_page(path=, lang=?, page=?)` → full value or `none` | fn (s) | D S R |
| (full value of a listed page) | `p \| deref` | F (s) | D |
| `.GetTerms "tags"` | `page.terms.tags`; `get_terms(taxonomy=, page=?)` returns `[]` for names that are not taxonomies (embedded `schema.html` uses `keywords`) | op/fn (s) | D R |
| `.Related .` on `site.Pages` / `site.RegularPages` | `related(pages=site.pages, page=?, indices=?, limit=?)` | fn (s) | D S |
| `.Param "k"` | `param(key=, page=?)` (page, then site) | fn (s) | D |
| `.Paginator`, `.Paginate list n` | `paginator()`, `paginate(pages=, size=?)` → `PagerView` (recorded; a conflicting re-call is an error) | fn (s) | S R T |
| `.Store.Set/Get` | `store_set(key=, value=, page=?)`, `store_get(key=, page=?)` | fn (s) | D |
| `.Content` etc. of another page in the content phase, `.RenderShortcodes` | `page_content(page=)`, `page_summary`, `page_plain`, `page_word_count`, `page_fragments`, `page_toc`, `render_shortcodes(page=)` | fn (s) | D |
| `.IsMenuCurrent`, `.HasMenuCurrent` | `is_menu_current(menu=, entry=, page=?)`, `has_menu_current(…)` | fn (s) | R |
| `$tax.ByCount`, `.Alphabetical` | `site.taxonomies.tags \| by_count`, `\| alphabetical` | F (s) | S R |
| `.Data.Singular/Plural/Term/Terms` | `page.taxonomy.singular/plural/terms`, `page.term.term` | op | S R |
| `.OutputFormats.Get "rss"`, `.AlternativeOutputFormats`, `.MediaType` | `page.output_formats.rss`, `page.alternative_output_formats`, `f.media_type.type` | op | D S R |
| the Go template object's `Version` / `Environment` / `IsProduction` / `IsDevelopment` / `Generator`; version compare | `build.version` (`"0.149.0-DEV"`), `build.environment`, …; test `is version_at_least(version=)` (semver; a `-DEV` build ranks below its release; checked against docs `new-in` pages) | op/T | D S R |
| `os.Getenv`, `os.ReadFile`, `os.FileExists` | `get_env(name=)`, `read_file(path=)`, `file_exists(path=)` (security allowlist) | fn | D |
| `path.Ext`/`Base`/`BaseName`/`Dir`/`Join`/`Clean` | `path_ext` `path_base` `path_base_name` `path_dir` `path_join(parts=)` `path_clean` | F/fn | D S R |
| `.Site.Config.Privacy.*` | `site.config.privacy.*` | op | embedded |
| `debug.Timer`, `debug.Dump` | removed during conversion; `dump` | –/F | D |
| (map order in templates) | `m \| sort_keys` (Go ranges maps sorted; literals keep insertion order) | F | – |
| (extra tests) | `defined` `undefined` `none` `string` `number` `map` `array` `starting_with` `ending_with` `containing` (bi), `matching` (tc) | T | – |

**Tera strings.** Tera 2.4 string literals know only the escapes `\n` `\t` `\r` `\"` `\'` `\/` `\\`; any other escape (`"\u{a0}"`, `"\s"`) is a syntax error. Write other characters literally (a U+00A0 character for Go's `printf "%c" 160`) and double the backslashes of regular expressions (`regex_replace(pattern="^\\s+", …)`).

**tera-contrib** supplies `regex_replace`, `urlencode`, `base64`, `filesizeformat` and `matching`. Its `date` filter is used only if T12 confirms that it produces Gregorian Thai month names (`th-u-ca-gregory`); otherwise ssg-locale registers `date` itself. `shuffle` is not registered, because no target site uses it and it needs the `rand` feature.

### 4.7 Conversion rules

These rules are generated into `template-api.md`. They are applied by hand; the optional migrate tool (T73) applies them too.

**Names and paths.**
- Files use v0.146 names (§4.1). Include and extends literals are lower-case, new-style names.
- `.Title` → `page.title`; `.Site.X` and `site.X` → `site.x`.
- Params are lower-cased: `.Site.Params.HomeTitle` → `site.params.hometitle`.
- Data keys keep their case: `item.Name`, `site.data.docs.config.baseURL`.
- Reserved front-matter keys stay available in params: `where … "Params.type"` → `[p for p in l if p.params.type == "snacks"]`.

**Missing values.**
- Printed params that may be missing → `page.params.x or ""`.
- Nested optional lookups use `?.`.
- Go `default` → `default_if_empty(value=)`. Do not use `or` for bools.

**Comparisons.**
- `eq $p $currentSection` → `p.id == current_section.id`. Pagers compare by `page_number`, dates by `.unix`.
- Mixed int/string comparisons get explicit `int` or `str`, for example R `ge site.params.yearcreate (now | date(format="%Y") | int)`.

**Control flow.**
- `{{ with X }}…{{ else with Y }}` → `{% if X %}{% set x = X %}…{% elif Y %}…`.
- `range $k, $v := m` → `{% for k, v in m %}`. Maps from views, data and filters are built sorted. Template literals need `| sort_keys` before ranging.
- `where` → list comprehension with `if`. `apply` → comprehension. `seq N` → `range(start=1, end=N+1)`.
- Variable reassignment inside a block → `set_global` (it is discarded inside includes).

**Templates and partials.**
- `define`/`block` in children → `{% extends "baseof.html" %}` plus `{% block %}`. Delete blocks the parent does not define.
- Partials → include, component or `partial()` (§4.3).
- `try` → `optional=true` on `get_remote` and `to_math`, or a `none` check.

**Formatting.**
- Go `printf` → `~`, `pad_start`/`pad_end`, `round`/`format_number`, `jsonify`, as in §4.6.
- Tera strings know only `\n \t \r \" \' \/ \\`: other characters literally (U+00A0), regex backslashes doubled (`"\\s+"`).
- Go date layouts → strftime (`"Jan 2, 2006"` → `"%b %-d, %Y"`).

**Removed Go-template idioms.**
- `range .Paginator.Pages` → `{% for p in paginator().pages %}`. Delete a second `.Paginate` that follows `.Paginator`.
- `{{ $noop := .WordCount }}` → delete.
- Another page's `.Content` inside a shortcode → `page_content(page=p)`.
- `.Scratch` / `newScratch` → `set`, `set_global`, `merge`; the page store only for cross-template flags.

**Components.** A component that calls site-bound functions passes `page=` or declares `@__nh`.

**Escaping.** `html`/`htmlEscape` → `html_escape`. In `<script>`, use `jsonify | safe`. In query strings, use `urlencode`.

**Assets and i18n.**
- Assets used with `execute_as_template` are Tera templates: `{{ .api }}` → `{{ data.api }}`.
- i18n files stay in the Go implementation's syntax, limited to `{{ . }}` and `{{ .Field }}` (§2.4 locale).


**Tera 2.4 syntax limits found in T35 (binding for every conversion, T60–T66):**
- No attribute or index access after a call or a parenthesised expression: `get_page(path=x).title`, `paginator().page_number` and `(x | f).y` are syntax errors. Bind first (`{% set p = get_page(path=x) %}{{ p.title }}`) or use `| get(key=…)` / `| get_path(path=[…])`.
- `}}` inside an expression ends the tag: write nested map literals with a space, `{"a": {"b": 1} }`.
- Functions receive no call position; pagination conflicts report the partial or layout the call ran in.

### 4.8 Template tooling

**Contract test** (`ssg-testkit::contract`, T02).
- It builds the contract instance from `spec::FUNCS` placeholders plus stubs for `spec::EMBEDDED_TEMPLATES`, then parses every template under `rust/sites/**/{layouts,assets,patches}` and `crates/layouts/embedded/**`.
- A second test snapshots `template-api.md` against `FUNCS`.
- So layout conversion can start as soon as T02 lands.

**`fugo templates check -s <site>`** (T37):
1. Loads the layouts and reports parse errors and unknown names, with Tera `ReportError` snippets (file:line:col).
2. Reports top-level names from `get_template_variables` that are not in the documented context for that template's role.
3. Lists lookup coverage for every (page, format) query of the site, and every shortcode and hook referenced by the content. It shows the chosen template and baseof, and "no match" rows.
4. Lints:
   - legacy layout names and literals;
   - site-bound calls inside components without `page=` or `@__nh`;
   - `partial(name="<literal>")` with arguments (should be a component);
   - content fields used in shortcode or hook templates;
   - Go-template markers;
   - map literals ranged without `sort_keys`.

---

## 5. Dependencies

Versions are from crates.io as of 2026-09-29. `(verify)` marks feature names or versions that T00 must confirm. T00 has network access, runs `cargo fetch --target x86_64-unknown-linux-gnu`, and commits `Cargo.lock`. Features are declared **only** in `[workspace.dependencies]`.

```toml
[workspace.dependencies]
ssg-workspace-hack = { path = "crates/workspace-hack" }
# data formats
serde        = { version = "1.0.229", features = ["derive", "rc"] }
serde_json   = "1.0.151"                       # default Map = BTreeMap; a test asserts no dep enables preserve_order
toml         = "1.1.6"                         # decode via toml::Table → base::Value
serde-saphyr = "1.3"                           # YAML 1.2; fallback serde_norway 0.9
csv          = "1.3"                           # (verify)
roxmltree    = "0.20"                          # (verify)
indexmap     = { version = "2", features = ["serde"] }
bitflags     = "2"
semver       = "1"
# templates
tera         = { version = "=2.4.0", features = ["no_fmt", "fast_hash", "preserve_order"] }
tera-contrib = { version = "0.3", default-features = false, features = ["regex", "urlencode", "base64", "filesize_format", "date"] } # (verify names)
# markdown & text
comrak       = { version = "0.55", default-features = false, features = ["attributes", "shortcodes"] } # (verify)
syntect      = { version = "5.3", default-features = false, features = ["default-fancy"] }
two-face     = { version = "0.5.2", default-features = false, features = ["syntect-default-fancy"] }   # (verify)
regex        = "1.13"
aho-corasick = "1.1"
html5gum     = "0.8.4"
unicode-normalization = "0.1.25"
unicode-segmentation  = "1.12"
unicode-properties    = { version = "0.1", features = ["general-category"] }   # (verify) sanitize by general category
percent-encoding = "2.3.2"
url          = "2.5"
rquickjs     = { version = "0.14", default-features = false, features = ["std", "parallel"] }  # SHOULD (A-D2): to_math → KaTeX (QuickJS-ng, MIT)
svgbob       = "0.7.6"                         # SHOULD (A-D2): diagrams_goat (Apache-2.0)
# time & locale (ICU4X, Unicode-3.0)
jiff         = { version = "0.2.37", features = ["serde"] }
icu_collator = "2.3"
icu_plurals  = "2.3"
icu_decimal  = "2.3"
icu_locale   = "2.3"
icu_datetime = "2.3"
# inflection (evaluated in T10 against the flect/prose oracle; own data tables if none reaches 100%)
# pluralizer / cruet / titlecase: (verify)
# files
walkdir      = "2.5"
globset      = "0.4.20"
mime_guess   = "2"
filetime     = "0.2"
# css / js / html output
grass        = { version = "0.13.4", default-features = false, features = ["random"] }
minify-html  = "0.18.1"
lightningcss = { version = "1.0.0-alpha.72", default-features = false }
oxc_minifier = "0.95"                          # lockstep with minify-html's oxc
# images
image        = { version = "0.25.10", default-features = false, features = ["jpeg", "png", "gif", "webp", "bmp", "tiff", "rayon"] }
fast_image_resize = { version = "6.1", features = ["image", "rayon"] }
webp         = { version = "0.3.1", default-features = false }   # lossy q, preset, sharp YUV (C libwebp, BSD-3)
kamadak-exif = "0.6.1"
imageproc    = { version = "0.27", default-features = false }
ab_glyph     = "0.2.32"                        # images.Text (T72a)
qrcode       = { version = "0.14", default-features = false }   # images.QR (T72a)
color_quant  = "2"
kmeans_colors = "0.7.1"
smartcrop2   = "0.4"                           # COULD
# hashing
sha2 = "0.11"
sha1 = "0.11"
md-5 = "0.11"
xxhash-rust  = { version = "0.8.19", features = ["xxh3", "xxh64"] }
base64       = "0.23"                          # same major as tera-contrib (verify)
# network (GetRemote)
ureq         = { version = "3.4", default-features = false, features = ["rustls", "gzip"] }  # (verify ring backend)
# concurrency, errors, logging
rayon        = "1.12"
dashmap      = "6"
thiserror    = "2.0.21"
anyhow       = "1.0.104"                       # cli only
tracing      = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter", "fmt"] }
# cli & serve (serve only in T71)
clap         = { version = "4.6", features = ["derive", "env"] }
notify       = "8.2"
notify-debouncer-full = "0.7"
axum         = { version = "0.8.9", default-features = false, features = ["http1", "tokio", "ws"] }
tokio        = { version = "1.53", default-features = false, features = ["rt", "net", "fs", "time", "sync"] }
# dev
insta        = { version = "1", features = ["yaml", "glob", "redactions"] }
tempfile     = "3"
pretty_assertions = "1"
dhat         = "0.3"                           # T33 allocation measurement (it binary only)
```

**External binaries**, run through `security.exec`:
- `esbuild` over the `--service` protocol. Its version is read from `esbuild --version`; it is pinned by `tools/esbuild/build.sh`.
- `postcss-cli` and the `@tailwindcss/cli` v4 CLI, installed by `tools/dev/node.sh` from a committed lockfile into `tools/dev/node_modules`. Both the Go and Rust builds use it.
- An optional `sass` (Dart Sass) binary, used when `transpiler = "dartsass"` is set.

**Not used:**
- giallo (EUPL-1.2; cannot emit Chroma classes);
- pulldown-cmark (the fallback only if T04 fails comrak);
- serde_yaml (deprecated), libsass, reqwest, tree-sitter grammars;
- imagequant (GPL-3.0), zenwebp (AGPL), nom-exif (non-standard licence);
- `arc-swap` (generations are frozen in `OnceLock`s);
- any Zola ≥ 0.22 source.

**Licence policy** (`rust/deny.toml`, cargo-deny format).
- `tools/dev/licence-check.sh` evaluates **SPDX expressions** from `cargo metadata --filter-platform x86_64-unknown-linux-gnu`, using Python stdlib.
- Allowlist: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, CC0-1.0, Unlicense, BSL-1.0, Unicode-3.0, CDLA-Permissive-2.0 (webpki roots), MPL-2.0 (lightningcss/cssparser, file-level copyleft).
- An `OR` expression passes if any branch passes. So `MIT OR Apache-2.0 OR LGPL-2.1-or-later` (r-efi) passes; `Apache-2.0 AND ISC` (ring) needs both.
- Bans: any EUPL, GPL or AGPL-only expression, plus the crates giallo, imagequant, zenwebp and nom-exif.
- The check runs in T00 and at every phase end. `cargo deny check licenses bans` can replace the script later without redefining the policy.
- Embedded templates (derived from the Go implementation, Apache-2.0) and non-cargo assets are recorded in `THIRD_PARTY/`: two-face syntaxes and themes, CLDR data, emoji data, livereload.js.

**Heavy-dependency isolation:**

| Heavy dependency | Isolated in |
|---|---|
| minify-html (oxc, lightningcss) | `minify` |
| image, webp (C) | `images` |
| syntect dumps | `highlight` |
| ICU data | `locale` |
| axum, tokio | `serve` |
| svgbob, rquickjs (QuickJS, C) | `funcs`, behind features `goat` and `math` |

---

## 6. Fate of the existing crates and the Go oracles

### 6.1 T00 order of operations

1. `git tag go-parity-final` on the I01 byte-identical state.
2. Move the retained fixtures and corpora (§6.3) to `rust/testdata/`. Convert them to plain JSON with this project's schema using `tools/dev/fixtures2json.py` (stdlib; unwraps the `goval`/`tval` type tags; re-run after any oracle regeneration). Update `sites.py` paths.
3. Move `crates/TERA_PLAN.md` to `docs/rust-port/TERA_PLAN.md`, with a header pointing to this plan.
4. `git rm -r crates/`, then `rm -rf crates/`. The second command also removes the ignored `crates/*/target` directories (`go-png/target` alone is 257 MB). Together they free 423 MB.
5. Delete the byte-exact Go oracle directories (§6.4).
6. Add `rust/target/`, `tools/dev/bin/` and `tools/dev/node_modules/` to `.gitignore`.

From then on, salvage uses `git show go-parity-final:crates/<crate>/<path>`. No legacy code remains in the working tree.

### 6.2 Salvage map

| Legacy crate(s) | Fate | Salvaged into (task) |
|---|---|---|
| go-value, go-unicode, go-strconv, go-fmt, go-sort, go-path, go-flate, go-html, go-json, go-url, go-image, go-png, go-hashstructure | delete | ecosystem crates (§5) |
| go-time | delete | the Go implementation's accepted date formats, expressed as jiff `strptime` patterns → `base` (T10); Go-layout → strftime table → `page::PermalinkPattern` (T21) and `migrate` (T73) |
| go-yaml, xtext-collate, goldmark, tdewolff-parse(-js), tdewolff-minify(-js), libwebp-sys, libsass-sys | delete | serde-saphyr, icu_collator, comrak, minify-html/lightningcss/oxc, webp, grass. Their corpora move to `testdata/corpus`. |
| gift | delete | **No transliteration.** Filters are built on `image::imageops` and `imageproc` (gaussian blur, median, `filter3x3`, rotate; T41 verifies names). Fresh code only for Sigmoid, Colorize, ColorBalance, Sepia, Pixelate (and Dither, COULD). Go's 16 resample names map onto fast_image_resize's 7 filters through a documented table, with a warning for approximations. The anchor-point maths → `images` (T41). |
| gotemplate | delete | `parse/{lex,parse,node}.rs` → front end of `migrate` (T73, COULD) |
| nh-common | delete | `paths/{pathparser,path,url}.rs` rules → `base::paths` + `vfs::PathParser` (T10, T20); `urls`, `kinds`, `htime` → `base` (T10); flect/prose rules → `base::inflect`/`title`, crate or own data tables (T10); `files` classifier → `vfs` (T20) |
| nh-allconfig, nh-config, nh-langs, nh-media | delete | default values and key names (as `impl Default`), config-dir and env merge rules, legacy-key table, environment-variable mapping, security policy, `hexec`, media and output-format tables → `config` (T13) |
| the old file-system crate | delete | mount semantics (lang, include/exclude, theme order, `_jsconfig`) → `vfs` (T20) |
| nh-parser | delete | page lexer rules, re-expressed as `lex` + `assemble` over `&str` → `pageparser` (T11) |
| nh-i18n | delete | message file formats, count extraction, fallback → `locale` (T12) |
| nh-page | delete | `page_frontmatter`, `page_matcher`, `permalinks`, `page_paths`, `pages_sort` → `page` (T21); `taxonomy`, `weighted` → `site` (T23b); `menu`, `pagemenus`, `pagination`, `pagegroup`, `related` → `nav` (T24) |
| the old site-build crate, nh-doctree | delete | capture, assembly, cascade, dates, build options → `site` (T23a); URLs, relations, taxonomies, `Dir()` retry → `site` (T23b); shortcode/summary/content rules → `render` (T34); render order, aliases, stats format, redirects → `build` (T36) |
| nh-markup | delete | hook contracts, TOC, autoid (github/github-ascii/blackfriday), attribute parser, alert parse → `markup` (T22), `highlight` (T25) |
| nh-tplimpl | delete | `templatedescriptor.rs` scoring rules re-expressed as `TemplateRole` + `Score` → `layouts` (T30); embedded list → T32; legacy mapping → T01 normaliser and `migrate` only |
| nh-tplfuncs | delete | sort-by-key with collator, set operations, HTML-aware truncate, urls, transform, lang, crypto, os policy → `funcs`, `sitefuncs` (T31, T35). Not `where`, `printf` or Scratch. |
| nh-resource(s), nh-resource-transformers | delete | metadata matching (`src` globs, `:counter`), naming, `integrity.rs` (copied; sha2/md5/base64), bundler, Sass import resolution + the `vars` import (`build:vars` today), PostCSS spawn + inline imports, GetRemote options, postpub per-field placeholders, jsconfig → `resources` (T40, T42) |
| nh-esbuild | **keep** `service/{protocol,client}.rs` (std only, our own Apache code); salvage `options.rs`, `resolve.rs`, `sourcemap.rs` | `esbuild` (T14) |
| nh-images | delete | spec grammar (`config.rs`), process semantics, filter-name mapping, overlay/mask/padding/opacity/auto_orient → `images` (T41) |
| nh-transform, nh-publisher | delete | canonify/relative URL replacers, minify config mapping, stats collector semantics → `publish` (T50) |
| nh-helpers | delete | URLize/MakePath/RelURL/AbsURL, word count/summary/reading time, filecache config → `base`, `markup`, `resources` |
| nh-commands | delete | flag mapping, build flow, fsync copy semantics → `cli` (T37), `publish` (T50) |
| nh-tpl, nh-deps | delete | – |

### 6.3 Fixtures and corpora moved in T00 (to `rust/testdata/`, converted to plain JSON)

**Needed by `sites.py`:**
- `nh-allconfig/tests/fixtures/load/<private site>/` (its configuration file)
- the old site-build crate's `tests/fixtures/build/*.json.gz`
- `nh-resource-transformers/tests/fixtures/getremote/getremote.json.gz`
- `go-image/tests/fixtures/site/*.jpg`
- `go-png/tests/fixtures/{repo,golden}/*.png`
- `nh-tplimpl/tests/fixtures/probe/probe.json.gz`

**Oracles** (`testdata/oracle/`):
- the old site-build crate's `{capture,assemble,content,site,build}`
- `nh-page/{paths,permalinks,pagination,menus,related,collections,frontmatter,summary,misc}`
- `nh-tplimpl/{lookup,store}`, `nh-allconfig/load`, `nh-media`
- `nh-parser/{pageparser,metadecoders}`, `nh-markup/{hooks,autoid,convert}`, `nh-i18n/{plural,translate}`
- `nh-resources`, `nh-resource-transformers/{jsbuild,postcss,tocss}`, `nh-images/{config,process,exif}`
- `nh-publisher/collector`, `nh-transform/absurl`
- `nh-commands/{e2e,cli,staticcopy}` (including `e2e/mini.txtar`), `nh-helpers/pathspec`, `nh-common/{paths,urls}`
- **kept for title and date parity:** `nh-common/{flect,prose,locales,cast,glob}`
- the xtext-collate Thai corpora

**Corpora** (`testdata/corpus/`):
- `goldmark/corpus{,-ext}.gmf.gz` (251 private-site bodies)
- `go-yaml/<private site>-fm.fixture.gz` (218 front matters)
- `go-time/{format,<private site>}.txt.gz`
- `tdewolff-minify/corpus/*`
- the golden stats file (stays under `tools/rust-port/golden`)

**How they are used.** Tests deserialise fixtures into `#[derive(Deserialize)]` structs and compare semantic fields only. Each crate keeps a reviewed `expected_diffs.toml` (YAML 1.1 booleans, CLDR collation, and so on).

### 6.4 Go oracles (`tools/go-oracle/`)

**Kept**, with output re-pointed to `rust/testdata/oracle/` (plus the `fixtures2json.py` step):
- the old site-build crate's fixtures, `nh-page/*`, `nh-tplimpl/lookup`, `nh-allconfig/load`, `nh-media/*`
- `nh-parser/{pageparser,metadecoders}`, `nh-markup/{hooks,autoid,convert}`, `nh-i18n/*`
- `nh-resources/*`, `nh-images/{config,process}`, `nh-publisher/collector`, `nh-transform/absurl`
- `nh-commands/*`, `nh-helpers/pathspec`
- `nh-common/{paths,urls,flect,prose,locales,cast,glob}`

These need only native Go; the arm64/qemu requirement ends.

**Deleted in T00:**
- `go-*`, `gift`, `goldmark`, `gotemplate`, `tdewolff-*`, `xtext-collate`, `libwebp-sys`, `libsass-sys`
- `nh-common/{compare,regexp,norm,values,math}`
- `nh-tplimpl/escdump`, `nh-doctree/walkmut`
- `tools/rust-port/i01/build-go-arm64.sh`

**Added in T01: `structure`.** It builds a site with the Go site-build package and dumps, for every (lang, page, output format):
- kind, path, outputs, target file name, permalink and rel permalink;
- the **chosen layout template and baseof**, as the Go implementation's own internal v0.146 descriptor path, so legacy names come out normalised;
- alias targets;
- (page, resource name) → rel permalink, including shared translation resources.

It also dumps each site's layout file list with normalised names, which T30 uses as its scorer input.

---

## 7. Testing and acceptance

### 7.1 Test layers

1. **Unit tests** inside modules, except in `view`, `sitefuncs`, `render`, `build` and `cli`. Each crate has one integration binary, `tests/it/main.rs`.
2. **Oracle-fixture tests** (`tests/it/oracle.rs`) on the §6.3 fixtures: semantic fields only, with `expected_diffs.toml`.
3. **insta snapshots:**
   - config decodes of the three sites;
   - the layout-lookup table per site;
   - markdown for about 150 curated docs and private-site bodies plus an adversarial set (html, toc, fragments, hook-call log with `page_inner`);
   - function outputs;
   - `template-api.md` against `FUNCS`;
   - embedded templates rendered against testsite views (in T60: `crates/cli/tests/it/embedded.rs`, a test-only overlay of the testsite);
   - the full testsite output tree (56 files: 55 in `public` plus the stats file; `crates/cli/tests/it/snapshots/testsite_output.snap`).

   Hashes, timestamps and versions are redacted. Snapshots are reviewed with `INSTA_UPDATE=always` plus `git diff`.
4. **Robustness corpora.** All 959 docs files and all 251 private-site bodies render without a panic, with idempotent heading IDs.
5. **Contract test and `templates check`** (§4.8).
6. **Site integration tests** (`ssg-build/tests/it/sites.rs`).
   - Built into a `MemorySink` and compared with the structure oracle: `mini.txtar`, the testsite, the private-site reconstruction and the edge-tree txtar sites.
   - Edge cases covered: cascade, i18n, aliases, term collisions, headless, `build` options, Thai paths, FM overrides, HTML content, Unicode shortcode params.
   - Docs is built only by the acceptance script.
7. **Determinism** (from T36): identical output-tree hash for `RAYON_NUM_THREADS=1` vs `=8`, and for two runs at `=8`.

### 7.2 Harness: manifests, structdiff, ratchet

All of this is Python stdlib under `tools/dev/`; there is no pip dependency.

- **One extractor for both sides.** `manifest.py` runs over the Go output and the Rust output alike. It uses `html.parser`, `xml.etree` and `json`, and reads PNG/JPEG/WebP headers for dimensions.
- **Two passes per site:**
  - The **minified** pass (I01 flags `--minify --clock 2026-09-27T12:00:00Z`) is used for L1 and L4.
  - The **unminified** pass is used for L2 and L3.
- **Build environment.** Both builds run with outbound HTTP disabled (proxy variables pointed at a refusing port) and with `tools/dev/node_modules` on the path. `compare.sh` deletes outputs unless `KEEP=1`.
- **Levels:**

| Level | Checks |
|---|---|
| **L1 paths** | Equal multisets of output paths after normalisation: `_hu_[0-9a-f]+` → `_hu_H`; fingerprints `.[0-9a-f]{16,64}.` → `.H.`; the known collision directories collapsed. Plus the structure oracle: per (page, format) the same target, permalink and chosen template; per (page, resource) the same rel permalink. |
| **L2 links** | HTML: same `<title>`, same `<link rel=canonical\|alternate>`, same set of normalised internal `href`/`src`/`srcset`. **Normalisation percent-decodes and NFC-normalises URL attributes** (Go's html/template percent-encodes non-ASCII; Tera does not). Alias → target map equal (including `page/1/` and `/en/`). RSS/sitemap/sitemapindex: same `<link>`/`<loc>`/`<guid>` lists. JSON: same key structure and URL leaves. `_redirects`/`_headers`: same line sets. **Link integrity:** every internal link in our output resolves to one of our files. |
| **L3 text** | Visible text after collapsing whitespace, decoding entities and mapping typographic characters to ASCII; a tag boundary separates words except a `span`'s inside `pre`/`code` (highlighter token spans, T66). Heading-ID lists per page. The stats file's tag, class and id sets. |
| **L4 assets** | Images: equal (width, height, format). Static files: byte-equal (sha256). CSS/JS: non-empty and referenced. |

- **Ratchet.**
  - `rust/testdata/baselines/<site>.json` stores each file's per-level status and a diff fingerprint.
  - A task may add or change entries only by listing them in `tools/dev/changes/<task-id>.md`, each with one triage class (`engine-difference`, `bug-fixed` or `accepted-deviation`) and a one-line reason.
  - An unlisted new diff fails the run. Baselines are reviewed at every phase end.
- **Self-test** (`selftest.py`): synthetic perturbations of the Go output, each checked for the right classification:
  - drop a file;
  - add a file;
  - change an internal link;
  - reorder attributes (must be ignored);
  - percent-encode a Thai href (must be ignored);
  - split a code element into token spans (must be ignored; T66);
  - change visible text;
  - change image dimensions;
  - change an RSS item link.
- **Tracked metric (A7).** Per-page visible-text equality ratio, reported with the worst 20 pages at every parity task.

### 7.3 Acceptance gates

**Docs patch variants** (T01 writes them as `tools/rust-port/i01/patches.json`, from `sites.py`'s `DOCS_*` lists; each layout entry maps to a Tera patch file under `rust/sites/docs/patches/<variant>/`):

| `sites.py` entry | i01 | reduced |
|---|---|---|
| remove `news/_content.gotmpl`, `shortcodes/x.md` (network), `functions/images/Text.md` (font GetRemote), the committed stats file | ✓ | ✓ |
| remove `shortcodes/qr.md`, `functions/images/{QR,Dither}.md` (COULD) | ✓ | ✓ |
| remove `quick-reference/syntax-highlighting-styles.md` (Chroma style gallery) | ✓ | ✓ (T72) |
| remove `shortcodes/highlight.md` | ✓ | – |
| `get-featured-image` Text → Grayscale; `header/qr.html`, `hooks/body-main-start.html` QR → Resize; `get-github-info.html` stub (DOCS_WRITE); smartcrop → anchors in content | ✓ | ✓ |
| baseof Tailwind → `minify`; Alpine/Turbo import stubs | ✓ | – (node.sh provides the modules) |
| passthrough off, emoji off, `codeFences = false`; render-codeblock / `hl` / code-toggle highlight → printf; remarshal → json | ✓ | – |

**Gates:**

| Gate | Site | Criterion |
|---|---|---|
| **A-T** | testsite + `testsite.txtar` | L1 56/56 (Go's 55 files in `public`, the reference `ssg-build/tests/it/testsite-go.txtar`, plus the stats file, which Go writes to the project directory and the reference does not hold) plus structure oracle; L2 all; L3 equal on every page (ratchet entries only `accepted-deviation`); stats-file sets equal |
| **A-R** | private-site reconstruction | L1 713/713 (712 files in `public` plus the stats file in the project directory; T61's count of 712 is `public` alone) plus structure oracle (incl. resource URLs); L2 all; A7 ≥ 0.95 with a clean ratchet. Must include: <ul><li>i18n with messages and Thai dates;</li><li>pagination (incl. 404 paging);</li><li>sitemapindex, the `/en/` redirect, robots;</li><li>the JSON output with `render-table.json.json`;</li><li>Sass via grass;</li><li>PostCSS purge reading stats (node.sh);</li><li>ExecuteAsTemplate TS assets (one file per target);</li><li>PostProcess per-field placeholders;</li><li>FM overrides, HTML content, content resources.</li></ul> R's `v1.html` inner rendering is `accepted-deviation`. |
| **A-D1** | docs, `--docs-patches i01` | L1 888/888 (887 in `public` plus the stats file) plus structure oracle; L2 all; heading-ID lists equal on every page; A7 ≥ 0.90 with a clean ratchet. Fences are plain `<pre><code>` (`codeFences = false`). |
| **A-D2** | docs, `--docs-patches reduced` | Working: <ul><li>Chroma-class highlighting (incl. `hl` inline/noClasses and `highlight.md`);</li><li>goat diagrams (`diagrams_goat`);</li><li>emoji;</li><li>passthrough + `to_math`;</li><li>`remarshal` in `code-toggle`;</li><li>Tailwind through `defer`;</li><li>real Alpine/Turbo `js_build`.</li></ul> L1 equal to the Go build with the same patches (889: 888 in `public` plus the stats file; `shortcodes/highlight.md` is kept); L2 all. Math and goat pages are `accepted-deviation` at L3. |
| **A-S** | real private site (needs the private repo, D6) | L1 path set equals `tools/rust-port/golden/canonical.sha256` (6,943 paths, normalised); static files byte-equal; L2 on 100 sampled pages |
| **A-DET** | all sites | byte-identical across `RAYON_NUM_THREADS=1/8` and across repeated runs |
| **A-P** | performance (goals, not gates) | **Release profile**, measured in T70 after `cargo clean` of the dev artifacts, with no agents active: docs cold build ≤ 1.5× the Go time on this machine, warm (image cache) ≤ 1.0×, peak RSS ≤ 1 GB |

**Allowed differences** (always listed in the ratchet, never silent):
- minifier bytes;
- highlight span structure;
- typographic characters versus entities;
- CSS/JS bundle bytes;
- deterministic term-collision winners;
- order of equal-weight Thai titles (CLDR version);
- the `generator` meta;
- KaTeX HTML → MathML;
- goat SVG bytes.

### 7.4 Converting the target sites' templates

- The Tera files live in `rust/sites/<site>/{layouts,assets,patches}` with **v0.146 names**. The private-site reconstruction's legacy files are renamed; the structure oracle already normalises the Go side.
- `sites.py --overlay rust/sites/<site> [--docs-patches i01|reduced]` generates each site as today, then:
  - replaces `layouts/`;
  - overlays `assets/` files (Tera versions of template-processed assets);
  - for docs, layers `patches/<variant>/`.
- Content, i18n, data and all other assets are shared unchanged. The Go side always builds the original, Go-patched layouts.
- A check (`sites.py patches --check`) asserts that `patches.json` and `rust/sites/docs/patches/**` correspond 1:1.

| Site | Files | Tera lines (est.) |
|---|---|---|
| testsite | 5 | ~100 |
| private-site reconstruction | 46 layouts + ~3 TS assets | ~900 |
| docs | 76 (44 partials, 23 shortcodes, 5 hooks, 7 top-level) + 11 patch files (2 variants) | ~3,600 |
| embedded (used ones only) | ~30 | ~1,200 |
| real private site | 58 | ~900 (private repo, T73) |

Each converted file is reviewed against §4.7 and the contract test. Faithfulness is measured with L2/L3 per page, and the worst 20 pages are reviewed in every parity task.

### 7.5 Commands

- Per crate: `cargo test -p <crate>`, run in the agent's own worktree.
- Phase end only: `cargo clippy -p <crate>` for each crate of the phase, and `tools/dev/licence-check.sh`.
- Acceptance: `tools/dev/compare.sh <site> [--docs-patches i01|reduced] [KEEP=1]`.
- Templates: `fugo templates check -s <site-dir>`.

---

## 8. Phased task breakdown

### 8.1 Rules for parallel agents

**Worktrees.**
- Each agent works in its own git worktree, `git worktree add ../wt/<task> -b task/<id> rust-port` (about 55 MB each).
- Every worktree sets `CARGO_TARGET_DIR=<main checkout>/rust/target`. Registry artifacts are shared. Workspace crates compile per worktree path; they are small.
- An agent never compiles another agent's half-edited crate. Only green commits are merged into `rust-port`, and a task rebases before merging.
- **Crate lock.** At most one agent edits a given crate at a time.
  - Parity tasks own only `rust/sites/**`, `rust/testdata/baselines/**` and `tools/dev/changes/**`.
  - A bug found in a crate becomes a short **fix task** that takes that crate's lock.

**Build commands.**
- Only `cargo test -p <crate>`, plus `-p` of direct dependants after an API change.
- Never `cargo check`, `clippy` or `doc` in the edit–test loop.
- Never `--workspace`; never `--release` before T70; never `cargo clean` (`cargo clean -p X` only when coordinated).
- `CARGO_BUILD_JOBS=4`.

**Disk budget and guard.** After T00 about 4.9 GB is free, and T01's `go clean -cache -modcache` frees about 2.2 GB more.

| Consumer | Planned |
|---|---|
| registry | ~0.8 GB |
| `rust/target` | ≤ 2.5 GB |
| `node_modules` | ~0.2 GB |
| worktrees | ~0.3 GB |
| site outputs (transient) | ~0.3 GB |

`tools/dev/disk.sh` fails if `rust/target` exceeds 2.5 GB **or** `df` reports less than 700 MB free. Every task reports `du -sh rust/target` and `df -h /` when it ends.

**Network.** T00 needs network for `cargo fetch`, and T01 for `npm ci` and GOPROXY (D8). Builds and tests afterwards run offline.

**Go.** T01 builds the native Go binary and the `structure` oracle once, keeps the binaries under `tools/dev/bin/` (gitignored), then runs `go clean -cache -modcache`.

**Clean room.**
- Implementing agents never open Zola ≥ 0.22 source; `scratchpad/zola-ref` is taken out of agents' reach at T00.
- Pre-3c9131db MIT Zola files (only if D2 = b) are fetched as single raw files, committed verbatim first with an MIT header, recorded in `PROVENANCE.md`, then adapted.

**Review checklist per task:** the §1.2 rules (typed model, no thread-locals, no Go-order emulation, no Go error texts, no Zola-isms), plus the task's own acceptance criteria.

### 8.2 Tasks

Sizes are Rust src + tests unless noted.

| ID | Title | Owns | Depends on | Acceptance | Size |
|---|---|---|---|---|---|
| **T00** | Bootstrap | `rust/{Cargo.toml,Cargo.lock,.cargo,clippy.toml,deny.toml,README.md,PROVENANCE.md,THIRD_PARTY/}`; stub crates with **real dependency edges**; `crates/workspace-hack`; `crates/testkit`; `rust/testdata/{oracle,corpus,site-assets}`; `tools/dev/{licence-check.sh,disk.sh,fixtures2json.py}`; `sites.py` fixture paths; `.gitignore`; TERA_PLAN move; deletion of `crates/` and obsolete oracles | – | <ul><li>`go-parity-final` exists</li><li>`sites.py` produces site inputs with the same file hashes as before</li><li>`cargo metadata --filter-platform …` resolves the acyclic graph</li><li>fixtures converted (record counts match)</li><li>`cargo test -p ssg-testkit` green (plain-JSON reader on 3 families, txtar)</li><li>(verify) items pinned in `Cargo.lock`</li><li>feature unification checked with `cargo tree -e features`</li><li>licence check passes on SPDX</li><li>`rust/target` < 400 MB</li></ul> | 1.8k |
| **T01** | Go oracle, patch variants, node tooling | `tools/dev/{oracle.sh,manifest.py,node.sh,node/}`, `tools/go-oracle/structure/`, `rust/testdata/golden/**`, `sites.py` (`--overlay`, `--docs-patches`, `patches.json`) | T00 (sites.py edits only) | <ul><li>testsite, reconstruction, docs-i01 and docs-reduced built natively with HTTP disabled</li><li>manifests committed (56/713/888, plus the reduced count; every count includes the project directory's stats file)</li><li>structure dumps (templates, baseof, targets, permalinks, aliases, resources) for 3 sites + `mini.txtar`; normalised layout lists</li><li>`patches.json` covers every DOCS_* entry</li><li>node.sh installs the pinned modules</li><li>20 Go-processed images in `golden/images`</li><li>Go caches cleaned; idempotent</li></ul> **State:** `rust/testdata/golden/` (schemas in its README): manifests and structure dumps of testsite, the private-site reconstruction, docs-i01, docs-reduced (56/713/888/889 files) and the structure dump of mini; `tools/rust-port/i01/patches.json` (26 entries); the Go binaries and the node modules live in the main checkout's `tools/dev/{bin,node_modules}` (`FUGO_TOOLS_BIN`, `FUGO_NODE_MODULES`) | 0.5k Go + 1.1k Py |
| **T02** | Template contract + testsite layouts | `crates/funcs/src/spec.rs` (then handed to T31), `rust/docs/template-api.md` (generated), `rust/sites/testsite/**`, `crates/testkit/src/contract.rs` | T00 | <ul><li>every §4.2 context and §4.6 name in `FUNCS` with kwargs, kind, phase, safety, site-bound flag and Go origin</li><li>`EMBEDDED_TEMPLATES` list</li><li>Tera facts verified and recorded: `@__nh` as an implicit name, `==` with an undefined final segment, `split`/`nth` kwargs, `?.`</li><li>5 testsite layouts converted; contract test clean</li><li>`template-api.md` snapshot equals `FUNCS`</li></ul> | 1.0k + doc |
| **T03** | structdiff, ratchet, self-test | `tools/dev/{structdiff.py,compare.sh,selftest.py,changes/}`, `rust/testdata/baselines/` | T01 | <ul><li>Go vs Go gives 0 diffs on 3 sites, both passes</li><li>self-test classifies all 8 perturbations correctly</li><li>an unlisted diff fails</li><li>`KEEP=1` keeps outputs</li></ul> | 1.5k Py |
| **T04** | comrak spike | `crates/markup` (spike; released before T22) | T00 | <ul><li>all 959 docs files and 251 private-site bodies run</li><li>per-feature verdict against normalised goldmark HTML: tight deflists (840), heading attrs, block attrs, fence attrs, math delimiters, alerts, emoji, linkify, typographer, raw HTML, `codeFences` plain</li><li>`sourcepos` accuracy for inline nodes</li><li>engine decision and list of custom passes in the crate README</li></ul> | 0.4k |
| **T10** | ssg-base | `crates/base` | T00 | <ul><li>`nh-common/{paths,urls}` and `nh-helpers/pathspec` 100%</li><li>flect/prose oracle 100% on the corpus and every S/R section and taxonomy name (pluralize, singularize, humanize, ordinalize, AP/Chicago/Go title case; crates evaluated first)</li><li>cast date formats; gobwas glob cases via globset</li><li>`docs.yaml` round-trips with `baseURL` and `Name` intact; R `ingredients_percentage` keeps `Name`/`Value`</li><li>Params folding; `IdVec`; path newtypes</li></ul> | 3.6k |
| **T11** | ssg-pageparser | `crates/pageparser` | T10 | <ul><li>`lex()` equals 141,869 items over 5,540 pages (kinds + byte ranges)</li><li>`assemble` tests with `InnerUse`</li><li>218/218 private-site front matters decode (`expected_diffs`)</li></ul> | 2.4k |
| **T12** | ssg-locale | `crates/locale` | T10 | <ul><li>translate fixtures ≥ 99%</li><li>message evaluator on R's `welcome`/`reviews`/`comments`; unsupported syntax is an error with file and key</li><li>collation sanity on `site-strings`</li><li>Gregorian `th` month names; locales oracle (`expected_diffs`)</li><li>decides on tera-contrib `date`</li></ul> | 2.0k |
| **T13** | ssg-config | `crates/config` | T10 | <ul><li>`nh-allconfig/load` values for docs, testsite, reconstruction and t24 sites</li><li>media tables equal</li><li>legacy-key table on a synthetic S-style config</li><li>env typing; `CliOverrides`</li><li>`[caches]` with `:cacheDir`/`:project`; privacy</li><li>error spans; insta snapshots</li></ul> | 3.5k |
| **T14** | esbuild client | `crates/esbuild` | T00 | <ul><li>`service/*` restored from the tag, std-only, version read at runtime</li><li>ping/build round trip</li><li>68 `jsbuild` cases produce the same module sets</li></ul> | 2.2k (1.3k moved) |
| **T20** | ssg-vfs | `crates/vfs` | T13 | <ul><li>path-parser oracle 100%</li><li>(file → lang, bundle kind, key) equal to the old site-build crate's `capture` fixtures for 3 sites</li><li>mount precedence tests</li></ul> | 1.8k |
| **T21** | ssg-page | `crates/page` | T13, T20 | <ul><li>`nh-page` fixtures ≥ 99%</li><li>cascade matcher</li><li>Go-layout permalink tokens → strftime</li><li>`capture_overrides`; `Markup` detection</li></ul> | 3.9k |
| **T22** | ssg-markup | `crates/markup` | T13, T04 | <ul><li>heading IDs 100% on the docs and private-site corpora</li><li>hook invocations and fields ≥ 98% after typographer normalisation</li><li>TOC equal; normalised HTML on ≥ 245/251</li><li>passes: deflist IDs, alert title/sign, block attrs, passthrough, emoji, linkify</li><li>`CodeFences::Plain` equals Go on the testsite fence and 20 A-D1 fences</li><li>context spans give the correct `inner_page`</li></ul> | 4.3k |
| **T23a** | site: capture + meta | `crates/site/src/{capture,tree,cascade,meta,filter,data}.rs` | T11, T12, T21 | <ul><li>capture/assemble fixtures 100%: page set per language, kinds, bundle roles, FM overrides (R `kind-override`, `lang-override` with `lang: TH`, `path-override`), duplicates, drafts/future/expired, cascade</li><li>`data::load` for D (5 files), S (nested JSON), R (`/` in keys), case preserved</li></ul> | 2.4k |
| **T23b** | site: nodes, URLs, relations | `crates/site/src/{nodes,urls,relations,taxonomy,translations,resources,refs}.rs` | T23a | <ul><li>site fixtures 100%: auto nodes, collections, term members, node dates, translations</li><li>**structure oracle: targets and permalinks per (page, format), resource URLs per (page, name)**</li><li>ref/get_page cases</li><li>segment-aware prefix lookup matches on all oracle sites (else `accepted-deviation`)</li></ul> | 2.6k |
| **T24** | ssg-nav | `crates/nav` | T23a (types; pageRef tests after T23b merges) | <ul><li>`nh-page` menus, pagination and related fixtures ≥ 99%</li><li>`related` with an explicit candidate list</li><li>alias plan equals structure-oracle aliases</li></ul> | 2.1k |
| **T25** | ssg-highlight | `crates/highlight` | T10, T22 | <ul><li>all ~3,900 docs fences highlight</li><li>`go-html-template` syntax (own MIT `.sublime-syntax`, or an alias)</li><li>classes ⊆ Chroma's, ≥ 90% token coverage</li><li>solarized-dark CSS</li><li>inline-style mode (`noClasses`), `hl_inline`, `lineNumbersInTable`, `linenos`, `hl_lines`</li><li>Chroma style names used by the docs layouts → themes, with a fallback warning</li></ul> | 1.8k |
| **T26** | ssg-minify | `crates/minify` | T13 | <ul><li>tdewolff corpora: no crash, idempotent, output re-parses</li><li>`[minify]` mapping documented</li></ul> | 0.7k |
| **T30** | ssg-layouts | `crates/layouts/src` | T20, T02 | <ul><li>`nh-tplimpl/lookup` (normalised): same winner 100%</li><li>**structure oracle: template and baseof equal for every (page, format) of the 3 sites** (scorer over T01's normalised lists)</li><li>legacy names rejected with hints</li><li>baseof variants, fallback prefixes, escaping by format, Go-marker detection, `uses_variable` after load</li></ul> | 3.0k |
| **T31** | ssg-funcs | `crates/funcs` | T10, T12, T02 | <ul><li>every pure `FUNCS` entry registered and snapshot-tested</li><li>agreement with `nh-tplfuncs` cases ≥ 95% (`sort_by`, set operations, `truncate_html`, humanize, urlize, plainify, jsonify, remarshal, `html_escape` on safe input, `default_if_empty`, `pad_*`)</li><li>`to_math` (feature `math`) renders all ~50 docs formulas</li><li>map outputs sorted; no dependency enables `serde_json/preserve_order`</li></ul> | 4.8k |
| **T32** | Embedded templates in Tera | `crates/layouts/embedded/**` | T30, T31 | <ul><li>rss, sitemap, sitemapindex, robots, alias</li><li>render-link/image/table/codeblock-goat</li><li>opengraph, twitter_cards, schema, pagination, google_analytics, `_funcs/get-page-images`</li><li>shortcodes figure, details, highlight, youtube, vimeo, instagram, x, qr, param, ref, relref</li><li>**parse-only** against the contract instance; rendering snapshots in T60</li></ul> | 1.2k Tera |
| **T38** | Walking skeleton (throwaway glue) | initial `crates/{view,render,build}` | T21, T22, T30, T50, T02 | <ul><li>builds the testsite into a `MemorySink` using a flat interim model and stub site functions</li><li>L1 reported (not a gate)</li><li>`Job`/`Output`/`Session`/`RenderScope` signatures frozen</li></ul> | 0.8k |
| **T40** | ssg-resources core | `crates/resources/src/{lib,store,meta,publish,remote}.rs` | T20, T26 | <ul><li>metadata matching and Get/GetMatch/Match/ByType vs `nh-resources`</li><li>fingerprint and SRI equal to Go</li><li>target-path identity: earlier language wins; conflicting inputs within a language raise an error</li><li>URL-token resolution incl. escaped forms and `&`/`'` in paths</li><li>`get_remote` with `[caches.getresource]` plus a key importer replaying the 51 cached YouTube responses</li><li>`inject_generated`</li></ul> | 2.8k |
| **T41** | ssg-images | `crates/images` | T13 | <ul><li>spec grammar and typed kwargs; dimensions 100% vs `nh-images/{config,process}`</li><li>PSNR ≥ 30 dB vs the 20 golden images; PNG alpha edges correct</li><li>**every `ImageFilter` variant except Text and Dither** (incl. Mask, Padding, Opacity, Overlay, AutoOrient, colour filters), with dimension parity</li><li>WebP q75 photo + sharp YUV</li><li>JPEG as Go's `image/jpeg` writes it (4:2:0, its tables; F9)</li><li>`[caches.images]`</li></ul> | 4.0k |
| **T42** | Resource pipes | `crates/resources/src/pipes/**` | T40, T14 | <ul><li>`to_css` compiles the reconstruction SCSS (slash division checked)</li><li>PostCSS with node.sh modules (R purge)</li><li>Tailwind compiles docs `styles.css` (cwd = project; `@import` via Vfs; `@source` of the stats file; `@plugin` from node_modules)</li><li>Babel spawn; `js_build` through T14</li><li>`post_process` per-field placeholders (R `head.html`); `execute_as_template` on Tera assets</li><li>a missing tool is an error naming the binary</li></ul> | 2.5k |
| **T50** | ssg-publish | `crates/publish` | T13, T26 | <ul><li>canonify: 100% of `nh-transform/absurl`</li><li>stats collector: 100% of `nh-publisher/collector` and golden stats</li><li>static-sync oracle</li><li>held outputs patched and re-scanned</li><li>URL-token extraction (entities, JSON escapes, srcset)</li></ul> | 2.2k |
| **T33** | ssg-view | `crates/view` | T23b, T24, T40, T22, T38 | <ul><li>Meta generation and Full generation per variant for 3 sites</li><li>Arc sharing (pointer equality in lists)</li><li>docs views allocate < 2× the Model (dhat in `it`)</li><li>every documented key printed for every kind</li><li>insta views</li><li>render-state types and `ContentRenderer` final; `sitefuncs::register` signature stub frozen</li></ul> | 2.6k |
| **T34** | ssg-render | `crates/render` | T33, T30, T31, T11, T22 (sitefuncs at T33's frozen stub) | <ul><li>shortcode semantics on R's edge pages: param typing, `inner`, `%` vs `<`, nesting, ordinal, parent, escapes, `arg` filter</li><li>per-page placeholder renumbering through `render_shortcodes`</li><li>`page_inner` spans</li><li>cross-page memo: cycle test and forced two-thread no-deadlock test</li><li>buffered store writes committed once</li><li>hooks via Tera; JSON variant</li><li>HTML content; bundled content resources</li><li>summary oracle</li></ul> | 3.4k |
| **T35** | ssg-sitefuncs | `crates/sitefuncs` | T33, T40, T41, T42, T12 | <ul><li>every site-bound `FUNCS` entry</li><li>get_page/ref/rel_ref cases</li><li>pagination recorder: first call, identical reuse, conflict error with both positions, pager N in wave 2 incl. inside `partial()`</li><li>frames: nested `return_value`; `partial_cached` caches values</li><li>defer; store; i18n; deref; components via `page=` and via `@__nh`</li><li>`get_remote` error and `optional`</li></ul> | 3.0k |
| **T36** | ssg-build | `crates/build` | T34, T35, T50 | <ul><li>full §3 pipeline: language sub-waves, wave 2, deferred wave, URL-token publishing, images</li><li>mini, testsite and edge trees in memory match the structure oracle</li><li>docs cross-page shortcode cases (`include`, `glossary-term`, `quick-reference`)</li><li>A-DET; collisions logged</li></ul> | 2.6k |
| **T37** | CLI + `templates check` | `crates/cli` | T36 | <ul><li>kebab-case flags with camelCase aliases (`--clean-destination-dir` / `--cleanDestinationDir`, `-s -d -b -e --minify --clock -D -E -F`)</li><li>environment variables</li><li>error report with positions; exit codes</li><li>`templates check` (§4.8) on 3 overlays</li><li>`nh-commands/cli` mapping</li></ul> | 1.4k |
| **T60** | testsite parity | `rust/sites/testsite/**`, baselines, changes | T37, T32, T02 | A-T; embedded-template rendering snapshots reviewed against Go; full-output insta committed. **State:** `crates/cli/tests/it/parity.rs` runs A-T through the binary: L1 56/56, L2 55/55 byte-identical (links, aliases, feeds, JSON URLs; dangling links only where Go's are), L3 every page, the stats file's sets equal the oracle-checked collector over Go's HTML (Go's file itself is not in the reference); the structure oracle waits for T01 (TODO in the test). Embedded snapshots and their review table: `crates/cli/tests/it/embedded.rs`, `crates/cli/README.md`; goat renders since T66 (`qr_code` since T72a) | fixes |
| **T61** | Reconstruction layouts + assets in Tera | `rust/sites/<private site>/**` | T02 (`FUNCS` final after T35) | contract test clean; v0.146 names; TS assets converted; redundant `.Paginate` dropped; §4.7 review | ~0.9k Tera |
| **T62** | Reconstruction parity | `rust/sites/<private site>/**`, baselines | T60, T61, T41, T42 | A-R | fixes |
| **T63** | docs layouts A | `rust/sites/docs/layouts/{top-level,_partials/**}`, `patches/{i01,reduced}/` for baseof, get-featured-image, qr, body-main-start, get-github-info | T02 | contract test clean for the base and both variants; patch files 1:1 with `patches.json` | ~2.3k Tera |
| **T64** | docs layouts B | `rust/sites/docs/layouts/{_shortcodes,_markup}/**`, `patches/i01/` for render-codeblock, hl, code-toggle | T02 | contract test clean; §4.4 rules (`quick-reference` → `page_content`); Scratch rewrites in `datatable` and `root-configuration-keys` | ~1.3k Tera |
| **T65** | docs parity (A-D1) | `rust/sites/docs/**`, baselines | T60, T63, T64, T41, T42, T14, T35 | A-D1. **State:** passed (`compare.sh docs-i01 --task T65`): L1 888/888 in both passes, S 1728/1728, L2 756/756, L3 748/750 with heading IDs equal on every page, L4 100/100, A7 0.9987; the two ratchet entries are `engine-difference` (a table on lazy list-item lines, the stats file's tags `?xml`/`=`; `tools/dev/changes/T65.md`). The Go GitHub stub now holds floats (golden docs-i01/docs-reduced regenerated) | fixes |
| **T66** | docs A-D2 | `diagrams_goat` in `funcs` (fix-task lock), markup/highlight/resources fix tasks, docs overlay | T65, T25, T42, T31 | A-D2 | **State:** passed (`compare.sh docs-reduced --task T66`; committed as `crates/cli/tests/it/docs.rs::gate_a_d2`): L1 889/889 in both passes, S 1730/1730, L2 757/757, L3 743/751, L4 100/100, A7 0.9907; ratchet entries: the three goat and three math pages and the stats file `accepted-deviation`, 21yunbox `engine-difference` (`tools/dev/changes/T66.md`). `diagrams_goat` uses svgbob (GoAT's size and `viewBox`); the CLI crate enables the `goat` and `math` features by default; `remarshal` YAML/TOML byte-equal to Go's on a fixture (yaml.v2 quoting and key order, go-toml literal strings, bare dates); the harness ignores code token spans (golden docs-reduced unminified manifest regenerated) | ~1k + fixes |
| **T70** | Cleanup, audit, A-P | `docs/rust-port/`, `tools/rust-port/`, `PROVENANCE.md` | T62, T65 | HANDOFF rewritten; specs marked "byte-parity sections obsolete"; licence and provenance audit; A-P measured (release) **State:** done. [`HANDOFF.md`](HANDOFF.md) rewritten (crate map, commands, gates, CI/CD, deviations, open items, PR summary); the old port's HANDOFF, TERA_PLAN and LAYER_CRITIQUE moved to `docs/rust-port/archive/`, every spec carries the "byte-parity sections obsolete" banner, `tools/rust-port/i01/{compare.sh,diff.py}` removed, `tools/rust-port/README.md` says what is in use. **A-P** (release, 4 CPUs, medians of 5–6 runs, HANDOFF §5): docs-reduced cold 3.29 s vs Go 3.99 s (0.82×), warm 3.26 s vs 3.51 s (0.93×), peak RSS 384 MB; testsite 0.071 vs 0.099 s; private-site reconstruction 0.62 vs 2.40 s cold. Fix: `ssg-highlight` links its syntax set in `build.rs` (it was 0.45 s of every process: testsite 0.50 → 0.064 s, docs 3.83 → 3.29 s); `FUGO_TIMINGS=1` prints phase timings. Audit: licence check and `notices.py` pass; `THIRD_PARTY/emoji/` added (listed but missing), PROVENANCE rows for `rust/sites/**`, `testsite-go.txtar`, `testdata/golden/**`; no Zola or unattributed copied code. Follow-up from T71: `build.is_server`/`site.server_port` set in the server (`BuildRequest::server`) | 0.3k |
| **T71** | ssg-serve | `crates/serve`, `cli` (serve) | T36 | memory sink; `/livereload.js` + `/livereload` WebSocket on the same port; notify debounce 1 s; full rebuild; static-only copy; edit → reload ≤ 2 s on testsite. **State:** `fugo server` (alias `serve`; `build`'s flags plus `-p --bind --append-port --disable-live-reload --live-reload-port -N --render-to-disk --no-http-cache -w --poll`, camelCase aliases; environment `development`). Memory sink per build, swapped in when the build succeeds (the last good build stays on failure, errors printed with positions); `--render-to-disk` serves the publish directory. Base URLs rewritten to the listener (Go's `fixURL`, one listener per language of a multihost site); the LiveReload script in every HTML page but aliases (`ssg-publish`, only for `server`: A-T stays 55/55); the Go implementation's `livereload.min.js` (MIT, `THIRD_PARTY/livereload`). Go file-server semantics (index, redirects, types from the media types, byte ranges), the `404.html` of the path's language with status 404. notify + notify-debouncer-full, 1 s (or `--poll`) over the project's and themes' mounts and configuration (their config files and `config/` dirs, any project-named, Go-program-named or `config.*` file appearing in the project or a theme): config → reload + rebuild; site → full rebuild; static only → changed files copied, no build; reload commands by the Go implementation's fast-render rules on the output diff (CSS in place, one path, full, none; `--navigateToChanged`). Tests: `ssg-serve` 6 unit + 15 `it` (testsite included), `fugo` `server::*` 3, `ssg-build` `skeleton::testsite_for_the_server`. Testsite, debug build: content edit → reload 1.54–2.03 s (typically 1.6–1.8 s: the 1 s debounce plus a ≈0.5–0.8 s rebuild, 92 % of which is `ssg_highlight::Highlight::new` rebuilding syntect's syntax set in every `Session::new`; caching it in `ssg-highlight` would give ≈1.1 s), static 1.0–1.1 s. Since then the syntax set is cached per process, and T70 links it at compile time and set `build.is_server`/`site.server_port`. Open: the browser error page, `[server]` headers/redirects, fast render, TLS, `--openBrowser` | 1.5k |
| **T72** | COULD features | per-feature crates (fix-task locks) | T65 | Each item lifts one patch and keeps A-D2 green: <ul><li>`images.Text` (`{op:"text"}`), `qr_code`, Dither, smartcrop (**T72a** implemented the first three in `images`/`resources`/`sitefuncs`; the docs patches are not lifted yet)</li><li>Chroma style gallery</li><li>`:git` lastmod</li><li>content adapters as a `_content.html` Tera template calling `add_page`</li><li>Org front matter</li></ul> | 3k |
| **T73** | ssg-migrate + real private site | `crates/migrate`, private repo branch | T62 | converter emits Tera with `TODO(fugo)` markers, renames legacy files and translates printf/where; after hand fixes ≤ 20% of lines changed on R; A-S | 2.5k + ~0.9k Tera |
| **T74** | docs-live: the published docs site (A-D3) | `sites/docs/**`, `tools/{legacy-docs,dev,rust-port}/**`, `testdata/golden/docs-live/`, baselines; engine items in `build`/`site`/`page`/`sitefuncs` (content adapters), `funcs` (KaTeX, GoAT), `images` (smartcrop), `markup`/`render` (goldmark tables, shortcode indentation), `highlight` (Chroma port) | T66, T72a | A-D3: the docs without patches against the published site (getfugo/getfugo.github.io at a1928152) — L1, L2, L4 equal on every file, A7 1.0. **State:** passed (`gate_a_d3`): L1 2373/2373, L2 776/776, L3 769/770 (Go's stats tokenizer reading `<?xml`/`<=` as tags), L4 873/873, A7 1.0; A-D1 and A-D2 reach A7 1.0 too (their math, GoAT and table entries `bug-fixed`); `tools/docs/build.sh` builds and serves the site | – |

### 8.3 Schedule (four lanes) and critical path

**Lanes.**
- Lane A: the model and the critical path.
- Lane B: templates, markup and render.
- Lane C: harness, output and resources.
- Lane D: layout conversion, images and nav. Lane D is mostly Tera writing plus the light crates, so it adds little CPU contention.

| Round | Lane A | Lane B | Lane C | Lane D |
|---|---|---|---|---|
| 1 | **T00** | – | T01 (Go program first; sites.py after T00 merges) | – |
| 2 | **T10** | T02 | T04 | – |
| 3 | **T13** | T11 | T03, T14 | – |
| 4 | **T20** | T22 | T12 | – |
| 5 | **T21** | T30 | T26, then T25 | – |
| 6 | **T23a** | T31 | T50 | T41 |
| 7 | **T23b** | T38 walking skeleton | T40 | T24 |
| 8 | **T33** | T32 | T42 | T63 |
| 9 | **T34** | T35 | fix tasks | T64 |
| 10 | **T36** | fix tasks | fix tasks | T61 |
| 11 | **T37** | overlay `templates check` dry runs | T71 | overlay fixes |
| 12 | **T60** | – | T71 (cont.) | – |
| 13 | **T65** (A-D1) | T62 (A-R) | – | – |
| 14 | T66 (A-D2) | T73 (if repo attached) | T72 | – |
| 15 | T70 | | | |

- **Critical path:** T00 → T10 → T13 → T20 → T21 → T23a → T23b → T33 → T34 → T36 → T37 → T60 → T65. That is 13 serial rounds. A-D2 follows in round 14 and cleanup in round 15.
- **No task shares a round with one of its dependencies.** The one exception is T01, whose `sites.py` edits land after T00 merges within round 1.
- **With three agents**, T41 moves to lane C in round 8 and T42 to round 9, T24 runs after T23b, and T63/T64/T61 slip to rounds 9–11. The critical path grows by about 2 rounds.
- **Early signals:**
  - the contract test (round 2);
  - the T30 template gate (round 5);
  - the T23b URL gate (round 7);
  - the walking skeleton (round 7).
- **Largest schedule risks:**
  - T34, the content engine. Assign the best-prepared agent, and write R's shortcode suites ahead of time.
  - The markup decision, settled early by T04.

---

## 9. Risks and mitigations

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| 1 | Late end-to-end signal; semantic drift found late | high | Oracle-fixture gates in every crate; structure-oracle gates in T23b and T30; walking skeleton in round 7; contract test from round 2 |
| 2 | Re-deriving the Go implementation's semantics loses subtle rules | high | Salvage rule by rule from `go-parity-final`, each with the fixture that covers it; the old port's specs; reviewed `expected_diffs.toml` |
| 3 | Tera 2 strictness (undefined values, keyword-only arguments, literal includes, hygienic components, no return values) and a young engine | high | <ul><li>views emit every key</li><li>the `or`/`?.` idioms</li><li>explicit `__nh` scope plus `page=`</li><li>`partial`/`return_value` frames</li><li>all names registered before load</li><li>`templates check` lints</li><li>pin `=2.4.0`; fix upstream (MIT)</li><li>minijinja is the documented escape hatch (costs a re-conversion)</li></ul> |
| 4 | Plain-data views: memory, depth, cross-page access | medium | Summary lists + `deref`; lazy full values; `page_content` functions with a precise error; dhat measurement in T33 |
| 5 | Concurrency: cycles, deadlocks, duplicated side effects | high | <ul><li>never-blocking memo cells</li><li>chain-based cycle detection in the scope</li><li>fragments as a separate stage</li><li>buffered store writes committed by the winner</li><li>no DashMap guard held while rendering</li><li>no parallel work inside renders</li><li>forced two-thread tests</li></ul> |
| 6 | comrak cannot handle tight deflists, `\[..\]` math, block attributes, alert titles or accurate inline positions | medium | T04 spike before the API freeze; engine-neutral `markup` API; pulldown-cmark plus custom passes as the fallback |
| 7 | Highlighting differs from Chroma; no `go-html-template` grammar; no `guessSyntax` | medium (visual) | Scope → Chroma class map with a coverage test; own Go-template syntax or an HTML alias; differences allowed at L3 |
| 8 | Publish-on-reference misses a resource or publishes too much | medium | Tokens decoded from HTML and JSON escapes; execute_as_template outputs scanned; rescan after `patch_held`; `publish` filter; L1 catches both directions |
| 9 | Pagination semantics | medium | Keyed recorder with conflict errors; `nh-page/pagination` oracle; A-R includes 404 paging and the conditional paginator |
| 10 | Disk: shared target, registry, node_modules, worktrees | high | §8.1 budget; `disk.sh` guards both target size and free space; heavy crates isolated; `[lib] test = false` on the heavy-graph crates; Go caches cleaned; release builds only in T70; `KEEP=1` opt-in |
| 11 | Cargo build-lock contention between four agents on 4 CPUs | medium | Lane D is mostly Tera; heavy crates stay out of lanes A/B until round 8; short `-p` test cycles |
| 12 | Crates needing a rustc newer than 1.94.1 | medium | `resolver = "3"` MSRV-aware resolution; `cargo update --precise` pins recorded |
| 13 | grass stale since 2024 | low for the targets | Reconstruction SCSS tested in T42; optional `dartsass` subprocess |
| 14 | Unverified crate details (tera-contrib features, comrak `attributes`, two-face, ureq TLS, qrcode, unicode-properties, base64 major) | low | T00 verifies and pins with network access; fallbacks named in §5 |
| 15 | Licence contamination | high (legal) | Clean room; SPDX-evaluated `deny.toml`; `PROVENANCE.md`; T70 audit |
| 16 | The ratchet quietly accepts regressions | medium | Required triage classes; self-test; hard invariants (link integrity, heading IDs, alias/RSS/sitemap sets, structure oracle); baseline review each phase |
| 17 | Hand-converted layouts diverge from the Go originals | medium | Per-page L2/L3 against Go; worst-20 review; conversion rules and lints; docs patch variants mirror Go 1:1 |
| 18 | Salvage turns into transliteration | medium | Legacy removed from the tree; `git show` only for rules; §1.2 checklist; clippy at phase end; filters built on ecosystem ops |
| 19 | Semantic drift from dependencies (YAML 1.2 vs 1.1, CLDR, ICU dates) | low | 218-front-matter corpus; locales oracle; collation and plural allowlists |
| 20 | External tools and network (esbuild, node, Tailwind, GetRemote) | medium | Pinned esbuild build script; node.sh lockfile; offline GetRemote cache with key importer; both builds run with HTTP disabled; clear errors naming missing binaries |
| 21 | Private site repo unavailable | medium | A-R is the blocking gate; A-S runs when the repo is attached (D6) |
| 22 | Scope creep toward all of the Go implementation | medium | MUST/SHOULD/COULD from the feature inventory drive the task list; COULD only in T72 |

---

## 10. Size estimate

| Crate | src | tests | Notes |
|---|---:|---:|---|
| ssg-base | 2,800 | 900 | ids, paths, anchors, inflect/title data |
| ssg-config | 2,800 | 700 | Value-tree pipeline + typed structs |
| ssg-vfs | 1,400 | 400 | |
| ssg-pageparser | 1,900 | 500 | |
| ssg-locale | 1,600 | 400 | incl. message evaluator |
| ssg-page | 3,000 | 900 | |
| ssg-site | 3,900 | 1,100 | T23a + T23b, incl. data loading |
| ssg-nav | 1,600 | 500 | |
| ssg-markup | 3,300 | 1,000 | custom passes, context spans |
| ssg-highlight | 1,400 | 400 | scope map, inline styles, Go-template syntax |
| ssg-minify | 500 | 200 | |
| ssg-resources | 4,000 | 1,300 | core 2.2k + pipes 1.8k |
| esbuild client | 1,800 | 400 | 1.3k restored unchanged |
| ssg-images | 3,200 | 800 | |
| ssg-layouts | 2,300 | 700 | + ~1,200 embedded Tera lines |
| ssg-funcs | 3,600 | 1,200 | incl. `FUNCS` spec |
| ssg-view | 2,000 | 600 | views + render-state types |
| ssg-sitefuncs | 2,400 | 600 | |
| ssg-render | 2,700 | 700 | |
| ssg-publish | 1,600 | 600 | |
| ssg-build | 2,000 | 600 | incl. site integration tests |
| fugo (cli + templates check) | 1,100 | 300 | |
| ssg-testkit | 1,000 | – | dev only |
| **Core total** | **≈ 55,900** | **≈ 14,900** | about 71k lines, roughly 1/6 of the old tree (337,648 src + 106,631 tests + 111k vendored C/C++) |
| Walking-skeleton glue (T38, replaced) | 600 | 200 | throwaway |
| ssg-serve (T71) | 1,500 | 300 | |
| COULD features (T72) | 3,000 | 700 | |
| ssg-migrate (T73) | 2,500 | 500 | |
| Harness (Python) + `structure` oracle (Go) | ~2,700 Py + ~500 Go | – | |
| Tera: testsite / reconstruction / docs / real private site | ~100 / ~900 / ~3,600 / ~900 | – | template lines |

**Throughput.** About 40 tasks of 0.4–4.8k lines across four lanes, in about 13 rounds to A-D1, 14 to A-D2, then the COULD tail. The main uncertainty is the parity rounds (T60, T62, T65); budget one to two extra rounds of fix tasks for them.

---

## 11. Decisions for the user

The plan proceeds with the stated default unless you say otherwise.

- **D1. Highlighting engine.** **Default:** syntect + two-face (MIT, emits Chroma class names, so the docs CSS keeps working). **Alternative:** giallo, which is EUPL-1.2 and cannot emit Chroma classes.
- **D2. Zola code reuse.**
  - (a) Clean room: ideas only.
  - **(b) Default:** additionally copy single MIT files from Zola before commit 3c9131db for low-risk infrastructure (serve/livereload/watch classification, fs copy-if-changed, image-queue shape), with `PROVENANCE.md` entries. This needs network access.
  - Code from Zola ≥ 0.22 is never copied.
- **D3. Delete the old port in T00.** It stays recoverable through the `go-parity-final` tag. **Default:** yes.
- **D4. Tera-only templates.** fugo stops accepting Go-template layouts, legacy layout names, and Go-template assets used with `execute_as_template`. Existing sites of the Go implementation convert them (the migrate tool, T73, is optional). Content shortcode syntax and i18n `{{ .Field }}` placeholders stay the Go implementation's. **Default:** yes.
- **D5. Accepted deviations from the Go implementation** (documented in `template-api.md`):
  - content is rendered before layouts, so another page's content inside a shortcode goes through `page_content`;
  - pagination is explicit, and a conflicting second `paginate` is an error (Go silently ignores it);
  - lazily published resources are published on reference (URL tokens);
  - no `#ZgotmplZ` sanitising;
  - YAML 1.2 in untyped params (`yes` stays a string);
  - deterministic winners for URL collisions;
  - newer CLDR collation;
  - the v1 shortcode semantics (the in-template version declaration) are dropped;
  - prefix lookup is segment-aware;
  - target-path assets: the earlier language wins, and conflicting inputs within one language are an error;
  - site-bound functions inside components need `page=` or `@__nh`;
  - `build.version` reports `0.149.0-DEV`.

  **Default:** accept.
- **D6. Real private site.** Attach the private repository for gate A-S and T73, or accept the reconstruction (A-R) as the final gate for that site. **Default:** A-R blocks; A-S runs when the repository is available.
- **D7. Scope beyond the three sites.** Whether `server` (T71) and the COULD features (T72) belong in this effort. **Default:** after A-D2, in the listed order; T71 may run in spare lane capacity from round 11.
- **D8. Network access.** Network is needed in T00 (`cargo fetch`) and T01 (`npm ci` for node tooling, GOPROXY for the Go oracle). **Default:** yes; everything afterwards runs offline.

---

## 12. Review notes

These are the review points that were rejected or only partly applied, one line each. Every other blocker, major and minor point from the three reviews was applied as written.

- **Thread-local `RenderFrame` stack** (completeness, feasibility): replaced by the explicit `__nh` scope value (Rust-style review). It covers the same cases (partial, shortcode, hook, defer, wave-2 pager) and stays sound under work-stealing.
- **Fixpoint rescan of published CSS/JS** (feasibility): rejected. The Go implementation publishes only through template calls, so we scan template outputs, `execute_as_template` results included, and nothing else.
- **Legacy layout-name aliases in the loader** (completeness, feasibility option): rejected. Overlays use v0.146 names, and the legacy mapping lives only in the oracle normaliser and `migrate`.
- **Keeping v1 inner semantics behind a Tera comment marker** (original draft): dropped per the Rust-style review. R's `v1.html` is an L3 `accepted-deviation` and does not affect the file set.
- **"A failed GetRemote returns `{err}` and is never a build error"** (completeness): rejected. Errors propagate unless `optional=true`; the converted docs templates use `optional=true`, and both builds run offline.
- **Chroma style → theme map for the full style gallery** (completeness, option 1): deferred to T72, and the gallery page stays patched in A-D2. `noClasses`, `hl_inline` and `lineNumbersInTable` were accepted into T25.
- **`resolver.feature-unification = "workspace"`** (feasibility, alternative): the hand-written workspace-hack was chosen, because the option is not known to be stable in cargo 1.94.1. T00 verifies, and we switch if it is stable.
- **Session typestate** (Rust-style, alternative): `OnceLock`-frozen generations were chosen instead. A typestate would make `sitefuncs` generic over two session types for no behavioural gain.
- **Scheduling T35 before T34** (feasibility, option 1): rejected. T34 builds against T33's frozen `sitefuncs::register` stub, and the docs cross-page cases move to T36, so both tasks share round 9 and the path stays 13 rounds.
- **Adopting an inflection/title crate wholesale** (ecosystem dossier): applied only as an evaluation step in T10. Own data tables are written if no crate reaches 100% on the flect/prose oracle, because `<title>` parity depends on it.
- **Removing `version_compare` in favour of a semver test** (Rust-style): applied. The `-DEV` ranking rule is checked against the docs `new-in` pages rather than assumed.