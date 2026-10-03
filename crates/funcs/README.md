# ssg-funcs

The template API (REWRITE_PLAN.md §4.6, §4.8): `spec.rs`, the single source of truth for every
Tera name (`spec::FUNCS`, the render contexts, `spec::EMBEDDED_TEMPLATES`; it generates
`docs/rust-port/template-api.md`), `register_placeholders` for the contract instance, `scan` for
the static checks Tera does not make, and (feature `runtime`, the default) `register_pure`: the
pure filters, functions and tests with the build-wide `PureEnv`, plus the tera-contrib subset.
`lib.rs` says how the build attaches them.

| API | What |
|---|---|
| `spec::FUNCS`, `spec::template_api_markdown()` | every name: kind, group, Go counterpart, kwargs, safety, site-bound or not |
| `register_pure(&mut Tera, &Arc<PureEnv>)` | every pure `FUNCS` entry; kwargs checked against the spec |
| `PureEnv` | clock, time zone, `Locales`, title and anchor styles, path options, `Diagnostics`, project directory, `EnvAllowlist` |
| `NOT_COMPILED` | the names whose feature is off (`to_math` without `math`, `diagrams_goat` without `goat`): registered as stubs that fail when called |

## Features

- `runtime` (default): everything but the spec.
- `math`: `to_math` (below). Dependencies: rquickjs (QuickJS-ng, C built by `cc`) and
  serde_json.
- `goat`: `diagrams_goat` (svgbob).

The `fugo` binary turns `math` and `goat` on (DEVELOPMENT.md, "Optional features").

## `to_math` (feature `math`)

Go's `transform.ToMath` with Go's output: KaTeX 0.16.22 and its mhchem extension, the release
the Go implementation bundles, run in QuickJS as Go runs it.

- **Engine** (`src/pure/katex.rs`). `assets/katex/katex.min.js` and `mhchem.min.js` are the npm
  package's dist files (verbatim, `THIRD_PARTY/katex/`); `assets/katex/render.js`, the entry
  point, is the Go implementation's `renderkatex.js` and `common.js` with one JSON message in
  (`{"expression", "options"}`, Go's `warpc.KatexInput`) and one out (`{output, warnings}` or
  `{err}`). Go runs the bundle as Javy-compiled WebAssembly (QuickJS 2024-01-13 through
  rquickjs 0.6) on wazero; fugo runs it natively in QuickJS-ng through rquickjs 0.14
  (MIT). An engine (about 25 ms to create, 2 MiB of heap, limited to 128 MiB) renders one
  formula at a time; idle engines wait in a process-wide pool, so render threads share them
  (Go: a pool of 8). KaTeX keeps no state between formulas but caches.
- **Options** (`src/pure/math.rs`): `to_math(options=?, optional=?)`. Go's defaults (output
  `mathml`, `minRuleThickness` 0.04, `errorColor` `#cc0000`, `throwOnError` true, `strict`
  `error`), then the `options` map decoded as `mapstructure.WeakDecode` decodes it into
  `KatexOptions`: keys match the field names exactly or case-insensitively, unknown keys and
  none values are ignored, `"true"`/`1` are booleans, numbers and booleans become strings
  (`"1"`/`"0"`), strings become numbers, `macros` is a map of strings (or an array of maps).
  `strict` must be `error`, `ignore` or `warn`.
- **Errors and warnings.** A formula KaTeX rejects (`throwOnError`, `\errmessage`) fails the
  render with KaTeX's message (`to_math: KaTeX parse error: …`); with `optional=true` (Go's
  `try`) it is a warning with id `to_math` and the result is none. `throwOnError: false`
  renders KaTeX's error markup instead. With `strict: "warn"`, KaTeX's warnings are warnings
  (`to_math: katex: LaTeX-incompatible input …`).
- **Cache.** The formulas of a build are kept by message (Go's `cacheMath`): one render and
  one report of the warnings per distinct formula and options. Go also keeps them in the file
  cache (`tomath/`); fugo renders them again in the next build (0.1–1 ms each).

### to_math fixture

`tests/fixtures/tomath.jsonl.gz` (read by `tests/it/math.rs`): 793 cases with Go's answer, each
`{"expression", "options", "output", "warnings", "err"}`; `tests/it/math.rs` requires the same
bytes, warnings and errors (Go's mapstructure messages, `err` = `decode: …`, only as errors).
The cases (`tests/fixtures/tomath-oracle/cases.js`): KaTeX's screenshotter corpus as the docs
render math and with the defaults, every formula of `testdata/legacy-docs/content` (its
passthrough delimiters), mhchem's manual, the options, errors, `strict` modes and weak decoding.
Regenerate with Go and the module cache of `go.mod` at `44529028`, and node with
`tools/dev/node.sh` installed (`yaml`):

```sh
T=$(mktemp -d); git archive 44529028 | tar -x -C $T/
mkdir -p $T/nhoracle/tomath && cp crates/funcs/tests/fixtures/tomath-oracle/main.go.txt $T/nhoracle/tomath/main.go
(cd $T && GOFLAGS=-mod=mod go build -o oracle ./nhoracle/tomath)
curl -sLo $T/ss_data.yaml https://raw.githubusercontent.com/KaTeX/KaTeX/v0.16.22/test/screenshotter/ss_data.yaml
NODE_PATH=tools/dev/node_modules node crates/funcs/tests/fixtures/tomath-oracle/cases.js \
  $T/ss_data.yaml testdata/legacy-docs/content | $T/oracle | python3 -c 'import gzip, sys; \
  sys.stdout.buffer.write(gzip.compress(sys.stdin.buffer.read(), 9, mtime=0))' \
  >crates/funcs/tests/fixtures/tomath.jsonl.gz
```

### Accepted deviations

- The messages of options that cannot be decoded are fugo's (``to_math: option
  `displayMode`: cannot parse "yes" as a boolean``), not mapstructure's; KaTeX's messages are
  KaTeX's.
- A key that matches two fields case-insensitively is decided by map order (Go: random).
  Case-insensitive matching is ASCII (Go's `strings.EqualFold` also folds e.g. `ſ` to `s`).
- `minRuleThickness` NaN or infinite is an error before KaTeX runs (Go's JSON encoder fails on
  it).
- No file cache across builds (above); in `fugo server` the warnings of a formula are
  reported in every build (Go: once per process).
- Engine limits differ (QuickJS-ng's 1 MiB stack and the 128 MiB heap limit against Go's
  32 MiB WebAssembly memory): a pathologically nested formula may fail at another depth.
