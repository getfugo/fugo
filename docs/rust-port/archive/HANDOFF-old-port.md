# Go version → Rust port: handoff (start here)

> **Historical (old port).** Paths under `crates/` refer to the byte-parity port that T00 of
> [`REWRITE_PLAN.md`](../REWRITE_PLAN.md) deleted; it is recoverable with
> `git show go-parity-final:crates/<path>` (commit `be02933a`, local tag). REWRITE_PLAN.md replaced §4 and the byte-parity rules; T70 archived this file. The current handoff is [`HANDOFF.md`](../HANDOFF.md).

This document is the entry point for continuing the Rust port of the Go version in a
new session, for example a Claude Code cloud session on Linux x86_64. It was
written when the first local session (macOS arm64) stopped on 2026-09-27.

## 1. Goal and acceptance test

Port the Go version (this Go repository, a fork of an older Go static site generator) to Rust. The acceptance test is
byte parity. The Rust binary must build the owner's private site
(a private repository, at a fixed commit) into output that
is byte-identical to the golden Go build: all 6943 files listed with their
SHA-256 in `tools/rust-port/golden/canonical.sha256`.

The golden was built with go1.27.1 on darwin/arm64 by
`tools/rust-port/build-site.sh` using these settings:

- `--minify --clock 2026-09-27T12:00:00Z`;
- the Go program's worker-multiplier environment variable set to 1 (deterministic; 13 taxonomy terms collide on one URL, and the last in tree order wins);
- a cold `resources/` cache;
- the checked-in YouTube GetRemote cache (the Go program's cache-directory environment variable set to `tools/rust-port/testdata/getremote-cache`).

The procedure was verified on a fresh clone and reproduces the golden 100%:

```sh
tools/rust-port/prepare-site.sh <private-site-repo> "$W"
tools/rust-port/build-site.sh <go-binary> "$W/<site>" "$W/out"
tools/rust-port/compare.py "$W/out"            # exit 0 == byte parity
```

The site repo is **private**. In a cloud session, attach
it as a second repository (the Claude GitHub App
needs access to it), then pass its local checkout path to `prepare-site.sh`
instead of the URL. The script clones it again and checks out the golden commit.

Rules:

- The site dir name keys the GetRemote cache.
- `npm ci` must use the site's lock file; the CSS bytes depend on postcss, cssnano, purgecss and caniuse-lite.
- js.Build needs esbuild **0.25.6** (`npm i esbuild@0.25.6`). The Rust port finds it through an environment variable naming the esbuild binary.

## 2. Read these first

1. `crates/README.md` holds the porting rules. Every crate is a faithful line-by-line port with
   Go-oracle differential tests. It also covers the FMA rule, the allowed crates, and the standalone crate layout
   (each crate has an empty `[workspace]` for now).
2. `docs/rust-port/specs/*.md` is the research. It covers every subsystem the private site's build uses,
   with Go file and line references and parity traps. `architecture-core.md` is the overview;
   `template-engine.md` §16 is the engine contract.
3. The layer design document in `crates/`, `crates/WAVE_B_PLAN.json` and `crates/GOTEMPLATE_CONTRACT.md` hold the
   site-layer design, its 31-task plan, and the template engine interface.
   `docs/rust-port/LAYER_CRITIQUE.md` is the review the design was revised against; all 21
   points were addressed.
4. Each crate's `PORTING.md` has its Go→Rust map, deviations, verification and gaps.

The specs and design docs mention paths such as `$SCRATCH/work/...`, `$W/...` and
`/private/tmp/.../scratchpad`. Those were the first session's scratch
directories. They do not exist anymore, and their large corpora (GBs) were
not committed. Regenerate anything you need with the Go oracles in
`tools/go-oracle/<crate>/`. The specs themselves are in `docs/rust-port/specs/`.

## 3. Environment

- **Go 1.27.1 exactly** for every oracle and for building the Go reference: `export GOTOOLCHAIN=go1.27.1`.
  - Unicode 17 tables, `encoding/json` on jsonv2 (default in 1.27), and the new jpeg DCT and flate encoder all depend on it.
  - go.mod's `go 1.23.0` line sets the GODEBUG defaults the go-url fixtures rely on, so do not change it.
  - The Go project's CI uses go1.25. `tools/go-oracle/go-unicode` is gated with `//go:build go1.27` so `go build ./...` still passes there.
- **Rust** with edition 2024 (≥1.85; the first session used 1.98). Build each crate with its own target dir, e.g. `CARGO_TARGET_DIR=/tmp/targets/<crate>`.
- **C/C++ toolchain** for `libwebp-sys` and `libsass-sys`, and for the cgo oracles (gift, go-hashstructure, libwebp-sys, libsass-sys).
- **Node + npm** for the site's postcss pipeline, plus esbuild 0.25.6.

### Platform caveat (important on Linux x86_64)

The golden was produced on **darwin/arm64**. Some Go outputs depend on the platform:

- The arm64 Go compiler fuses `x*y+z` into FMA and amd64 does not. This affects gift resampling (11 of the golden images), the flate EstimatedBits choice and tdewolff ParseFloat.
- arm64 and amd64 give different results for `int64(+Inf)` and for the `slices.Min`/`Max` NaN payload.
- libwebp's DSP paths and libm, and libsass's number formatting, depend on the C toolchain.

The Rust ports replicate the **arm64** behaviour explicitly (`mul_add` etc.), and every checked-in fixture came from arm64 Go. So `cargo test` is meaningful on any machine. **But do not regenerate float-sensitive fixtures (gift, go-flate, tdewolff strconv) with Go on amd64.** The fixtures would change even though the port is right.

To produce arm64 Go output on Linux x86_64, build the oracle for linux/arm64 and run it under `qemu-aarch64-static` (`apt install qemu-user-static`). linux/arm64 Go reproduces the checked-in darwin/arm64 fixtures of gift (all of them) and go-png. Pure-Go oracles need only `GOARCH=arm64`. cgo oracles additionally need a C cross-compiler; zig works (`zig cc -target aarch64-linux-musl -UNDEBUG`, with `-ldflags '-linkmode external -extldflags -static'`; `zig cc -O2` defines `NDEBUG` where cgo's clang does not, which changes LibSass's behaviour on invalid UTF-8). The recipe is in `crates/gift/PORTING.md` (§FMA sites, §Regenerating fixtures).

The first Linux x86_64 cloud checks (go-value, go-html, go-path, go-sort, go-unicode, go-time, tdewolff-parse) passed with 0 real mismatches over about 20M fresh cases. The only differences were the Go platform differences listed above. libwebp-sys and libsass-sys pass on linux/x86_64 when compiled by clang with `-ffp-contract=on -mfma`, which their `build.rs` now selects off Apple (see their PORTING.md). gcc gives different bytes: FMA contraction in libwebp, argument evaluation order in LibSass.

## 4. Crate status (2026-09-27)

Legend:

- **verified**: ported, red-teamed and committed.
- **ported**: complete; the red-team was interrupted or never run.
- **partial**: work in progress.

| crate | ports | status | evidence / next step |
|---|---|---|---|
| go-value | value model (`interface{}`+reflection), time data | verified | used by everything; has a NilKind registry for named Go types |
| go-unicode | unicode (U17), utf8, utf16, strings/bytes | verified | exhaustive per code point, 9.2M strings vectors |
| go-strconv | strconv | verified | all 2^32 float32 patterns, Go test tables |
| go-time | time | verified | ~538k checks + 1,202 zones |
| go-url, go-html, go-path, go-sort | net/url, html, path(+filepath), sort/slices | verified | ~15M adversarial cases; Linux-checked |
| go-yaml, go-hashstructure | yaml.v2 + metadecoders, hashstructure | verified | 368k YAML inputs × 4 paths |
| go-image | image, color, draw, jpeg | verified | all 538 site JPEGs, ~4M cases |
| xtext-collate | x/text collate + CLDR 23 tables | verified | 30k tags, Thai enumerations |
| tdewolff-parse | tdewolff/parse (minus js) | verified | ~21M fuzzed streams; Linux-checked |
| go-flate | compress/flate + zlib | verified | 6,676 cases + 11 golden PNG IDAT streams; red-team: fuzz seeds 4–15 (60k, arm64), compressor sweeps (Flush/Close at every position, 42 dictionary sizes, failing sinks; 269k), 12.35M truncation/bit-flip decodes, 12k `Reset` chains, all 65,536 zlib headers, Adler-32 boundaries; 0 differences; 16 checked-in inputs now depend on the `EstimatedBits` FMA sites (each site mutation-killed; amd64 Go differs on 12); Go's level 7–9 `NewWriterDict` stored-block quirk reproduced on purpose; see `crates/go-flate/PORTING.md` |
| go-fmt | fmt over Value | verified | 4.49M Sprintf outputs; red-team (third pass, report in PORTING.md): ~6.0M cases + 759M single-operand matrix outputs (every flag order, `*`/`[n]` widths, bad indexes, ~90 verbs, nil-receiver hooks, registered named methods, Sprint spacing), 0 differences after 2 fixes: a stack guard for deeply nested values (Go handles 100k levels), and `Object::underlying` (new, in go-value) so named basic types (`time.Month`, `hstring.HTML`, …) print like Go; arm64 and amd64 Go agree on every fixture |
| go-json | encoding/json (jsonv2-backed v1) | verified | red-team: 10.45M cases over 11 modes (adversarial, exhaustive 1–3-byte texts and every rune, 20k-digit numbers, 4–400 KB docs, random read chunking, site-shaped values incl. the real `maps.Params`, 171k on arm64 via qemu), 0 differences after one fix (`omitzero` on `maps.Params` now calls its `IsZero`); the `advtext` hang was Go's own `json.Indent` looping forever (non-blank prefix, empty indent), now guarded exactly in the oracle and documented as deviation 9 |
| goldmark | yuin/goldmark v1.7.12 + extensions | verified | CommonMark spec, full private-site corpus, 8,658 ext edge cases; red-team: ~97M records (78.6M exhaustive token strings over 8 alphabets, unicode/labels/tabs/shortcode-mix fuzz, every Unicode P/S/space rune in flanking/linkify/typographer patterns, all HTML5 entities, 100 KB–5 MB docs), ~54M with AST dumps (Lines, attributes, Text, per-heading TOC renders), 0 output differences; fixed 4 stack overflows on deep input (attribute parsing, attribute drop, `{id=[…]}` panic message, `Node.Text`) that Go handles; `AttrValue` now has a `Drop` impl (match it by reference) |
| tdewolff-minify | minify (html/css/json/svg/xml) | verified | ~114k checked-in records; red-team: 36.1M records (html incl. every tag/attribute and random option sets, site-shaped pages, css incl. all colours, svg, xml, json, units, bad bytes, every prefix of 4M truncations, 64 KiB–2 MiB docs), 0 differences from arm64 Go (amd64 disagreements re-answered under qemu); fixed a stack overflow on deeply nested CSS functions (Go handles ~1M levels); nested minifier calls (iframe, `text/html` script/style, data URIs) still recurse ~2.2 KB/level, and a Go panic (`url:local('`) is reproduced on purpose; see §5 item 3 |
| tdewolff-parse-js | parse/js | verified | 33k inputs × 8 dump modes; red-team: ~12.1M inputs × 8 dump modes (grammar, reparse of every minify generator, 3,830 real JS files / 86.6 MB, exhaustive short byte/token/whitespace sequences), 0 differences; arm64 oracle (qemu) reproduces every fixture; `ExprStmt.String` deviation (Go is exponential in nesting, same bytes) |
| libwebp-sys | vendored libwebp 1.3.2 + gowebp wrapper | verified | object code identical to cgo per unit; 807/807 golden webps; linux/x86_64 passes every test when built with clang `-ffp-contract=on -mfma` (the `build.rs` default off Apple); red-team: option sweep (30 qualities × 20 presets), adversarial geometry and `Pix` lengths, out-of-bounds-read detector, 356,600 cases + 200k fuzz, 0 differences after 1 fix (a >16383-px picture with a short `Pix` now fails with Go's "failed to encode") |
| libsass-sys | vendored libsass 3.6.6 + golibsass wrapper | verified | 2,052 cases; linux/x86_64 passes every test with clang (the `build.rs` default off Apple); red-team: error translation (every `@error` value kind, invalid UTF-8, resolver errors at random positions), styles, precisions, source maps, ~560k cases + 1M JSON documents, serial and parallel, 0 differences after 3 fixes (compiles run on an 8 MiB thread like cgo, so deep recursion no longer aborts; a resolver panic stops the compile like Go; iterative JSON validation); `%q` now uses go-strconv |
| go-png | image/png | verified | Go reader/writer/paeth tests ported; 217,650 checked-in differential checks (synth, filter ties, pooled encoders, 75 real files, 42,590 truncations + 42,590 reader failures, 9k generated/mutated PNGs) + 1.56M out of repo (`GO_PNG_BIG`), 0 differences; 11 golden PNGs re-encode byte-identical; fixtures from an arm64 (qemu) oracle build, identical to amd64; see `crates/go-png/PORTING.md` |
| gift | disintegration/gift + the Go implementation's filters | verified | all 61 Go test functions ported; 603 site images identical (first session); every FMA site read from the arm64 disassembly; red-team: ~4M cases on the arm64 oracle (synth, params, DrawAt, far-origin coordinates, site-shaped chains, every resampling kernel, bounds, Go panics), 0 differences after 2 fixes (Go-wrapping integer arithmetic in anchorPt and near the int64 range ends, which panicked under overflow checks); FMA mutants killed by checked-in tests now 88 of 125 (25 equivalent, 12 unobservable) |
| tdewolff-minify-js | minify/js | verified | 783 upstream table rows, 58k checked-in fixture checks (+2×495k out of repo, 27.5 MB corpus × 6 configs) identical; 1,589 HTML docs through the Go implementation's full minifier; red-team: ~16.8M checks (literals, logic rewrites, numbers at precisions 0–21, token soup, programs, real files × 13 configs, `<script>`/`on*` through the Go version's HTML minifier, exhaustive short sequences), 0 differences; no FMA sites (objdump) |
| gotemplate | forked text/template + html/template | verified | host contract (`GOTEMPLATE_CONTRACT.md` C1–C13) implemented and documented in `crates/gotemplate/PORTING.md`; Go's lex/parse/exec/escape/content/clone/multi/template tests ported; oracles: 4,873 parse + 13,470 site-like exec + 23,542 html exec operations, 39k escaper calls, 54k transitions; the escaper reproduces Go's escaped trees of all 120 repo layouts byte for byte. Red-team: 800k parse, 640k exec, 545k builtin value pairs, 28k printf, 400k html scripts (2.9M ops), 500k namespace scripts (5.3M ops), 80k mutated repo layouts, 0 differences outside documented deviations after 4 fixes (nil `any` host results, `TryError`/`ExecError` modelling); **the private site's layouts (private repo) are not yet in the escdump corpus**; nh-tplimpl (T13) can now replace its `engine.rs` placeholder |
| nh-* (25 crates) | the site layer | skeleton | all `cargo check`; 1,121 `todo!()`; ownership per `WAVE_B_PLAN.json` |

## 5. What to do next (in order)

1. **Finish Wave A.**
   - Port `gotemplate` (text/template, then html/template) against the contract. This is the long pole; every site-layer task needs it.
   - Finish `tdewolff-minify-js`. The whole-page golden check for the minify stack needs it.
   - ~~Write tests and PORTING.md for `go-png`.~~ Done.
   - ~~Write PORTING.md for `gift` and port its unit tests.~~ Done.
2. **Red-team the "ported" crates.** Use an independent pass that extends the Go oracle with adversarial and random inputs and fixes any divergence. The first session found and fixed real bugs this way (go-time, go-yaml, xtext-collate).
3. **Wave B, the site layer.**
   - Follow `crates/WAVE_B_PLAN.json`: 27 port tasks with disjoint module ownership, then 4 integration tasks.
   - Each task has concrete oracle and acceptance tests. The critical path is in the layer design's §12.
   - Wire the Wave A dependencies into the nh-* `Cargo.toml` files as each lands.
   - Deep input: Go grows goroutine stacks to 1 GB, and the Rust ports recurse wherever Go does in nested minifier calls, the JS parser/printer and gotemplate exec. Run minification, JS processing and template execution on threads with large stacks (the JS crates and tdewolff-minify tests use 1 GiB; see each PORTING.md "stack" notes).
   - Named basic types (`time.Month`, `time.Weekday`, `time.Duration`, `hstring.HTML`, `VersionString`, …): hosts must implement go-value's `Object::underlying` for them (nh-common's `hstring.HTML`, `maps.ParamsMergeStrategy` and css types do). go-fmt, gotemplate (truthiness, `len`, builtin comparisons) and go-json (marshal, omitempty/omitzero) read it; go-hashstructure does not yet, and T18's `eq`/`lt`/… must.
   - Go map order leaks into output in a few places, and the port uses sorted order there. Known cases: GroupBy on non-int/string keys, TaxonomyArray and full ties in related results (T12), and layout lookups whose candidates tie on the first weight but differ on the later ones (Go's `bestMatch.isBetter`; T13 counted 2 such lookups in real builds of its synthetic modern site). When I01 compares against the golden private-site build, check any layout-choice mismatch against this list first; Go itself is nondeterministic in these cases.
   - OS directory order leaks too. The golden build ran on APFS, which lists directories sorted; ext4 does not. Known cases are the `_jsconfig` auto-mounts (T09), static/`RootMappingFs.ReadDir` listings (T05), and anything else that walks with `os.ReadDir` order. Tests compare these as sets. For I01 on Linux, sort where Go relies on the OS order, so that the output reproduces the darwin golden build.
   - The host machine leaks in too. When the media-type config has no match, `resources` falls back to Go's `mime.TypeByExtension` (T14's `mime` module). That reads the system globs2 and mime.types files, so the result depends on the machine: the golden build used macOS's tables. For I01, pin the fallback table to the golden machine's result if any private-site resource reaches it.
   - Oracles must never write into the repository tree. T14's first docs fixture recorded the stats file in `docs/` that a Go build had clobbered. Build sites from a temp copy, or with an in-memory publish/working dir.
   - Go panics that the ports reproduce on purpose (for example tdewolff-minify's `url:local('` in CSS, `css/css.go:748`): a Go build crashes on them, so the site layer must decide whether to let the panic propagate (parity) or catch it per page.
4. **Integration.**
   - Merge all crates into one root Cargo workspace (drop the per-crate `[workspace]` tables).
   - Build the binary (`nh-commands`).
   - Iterate with `build-site.sh` + `compare.py` until 6943/6943 match.
   - Classify each remaining diff by subsystem and fix it in the owning crate with a new oracle test.

## 6. How work was run, and cloud notes

- The first session ran ports as parallel agents, one crate each, followed by an independent **red-team agent** per crate. Every result was committed only after the crate's tests passed.
- A cloud machine has 4 vCPUs, so run fewer agents at once, and give each crate its own `CARGO_TARGET_DIR`.
- Commit to `rust-port` often; nothing outside git survives the session.
- Pushing needs GitHub write access to the project's repository. Without the Claude GitHub App installed on the org, cloud pushes fail with 403. As of 2026-09-27 evening, cloud pushes to `rust-port-cloud/*` worked.
- Cloud verification branches: `rust-port-cloud/linux-verify-4` (extended xtext-collate/go-image Linux verification) is merged into `rust-port`. The `tags-fuzz.tsv` overlap was resolved in favour of `rust-port`'s original fixture; see `crates/xtext-collate/PORTING.md`.
- `tools/go-oracle/*` are `package main` programs inside the Go version's Go module. Keep them gofmt- and vet-clean, because CI runs `go vet`/`go test ./...` on the whole module.
