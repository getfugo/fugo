# ssg-dev

The repository's tools, one binary: `cargo dev <command>` (the alias of `.cargo/config.toml` for
`cargo run --quiet --locked -p ssg-dev --`). None of it is part of the `fugo` binary.

| Command | What | Used by |
|---|---|---|
| `sites list`, `sites make <site> <dir> [--docs-patches i01\|reduced\|live] [--overlay sites/<site>]`, `sites cache <site> <dir>` | the end-to-end sites (the legacy docs site with its patch variants, the testsite, mini, images, errors, probe, the T24 sites), written outside the repository, with the Tera overlay; the `--cacheDir` contents a site needs | `tools/dev/compare.sh`, `tools/legacy-docs/build.sh`, the ignored real-site tests (`FUGO_SITES`) |
| `sites patches [--check]` | `tools/rust-port/i01/patches.json` in its canonical form, checked against `sites/docs/patches/` | by hand after editing patches.json (a test checks it) |
| `manifest extract <publish-dir> …`, `manifest summary <file>…` | the manifest of a build (L1–L4; schema in `testdata/golden/README.md`) | `compare.sh` |
| `structdiff compare …`, `structdiff changes` | the comparison of two builds and the ratchet (`tools/dev/changes/README.md`) | `compare.sh` |
| `selftest [--go-out <dir> [--project <dir>]] [--keep]` | structdiff's self-test: perturbations of a Go build's output, each classified exactly | by hand with a real output (a test runs it on Go's testsite output) |
| `licence-check [-v]` | the licences of the dependency graph against `deny.toml` | CI's Lint job |
| `notices <triple> <file>` | `THIRD_PARTY_NOTICES.txt`: the licences of the crates linked into `fugo` for a target | CI's Build job |
| `package <binary> <triple> <out-dir> [<notices>]` | the release archive (`.tar.gz`, `.zip` for Windows) and its `.sha256` | CI's Build job |
| `file-length` | fails when a file of code is longer than 500 lines (AGENTS.md) | CI's Lint job |
| `bench <binary> <results.json> [--runs <n>] [--go <binary>]` | build time and peak memory of the docs site and generated sites, fugo's and the Go implementation's (DEVELOPMENT.md, "Benchmarks") | CI's Benchmark job |

The module documentation has the details (`src/sites.rs`, `src/manifest.rs`, `src/structdiff.rs`,
`src/selftest.rs`, `src/licence.rs`, `src/notices.rs`, `src/package.rs`, `src/file_length.rs`,
`src/bench.rs`). Release versions are
`tools/dev/version.sh` (POSIX sh: `bump.yml` and `image.yml` run it without a Rust toolchain).

## How it is built

Until v1.0.0 these tools were Python scripts (`tools/dev/*.py`, `tools/rust-port/i01/sites.py`;
`git show v1.0.0:<path>`), which wrote the golden manifests of `testdata/golden/`. ssg-dev is
written with Rust libraries rather than translated from them:

| Module | What | Libraries |
|---|---|---|
| `src/scan.rs` | what a manifest reads from a page: title, `rel` links, URLs, alias target, visible text, heading ids; the items of feeds and sitemaps | html5gum (the element's text state set as a browser's tree builder sets it: `script`, `style`, `textarea`, `title` …), quick-xml |
| `src/urls.rs` | internal links as site paths, percent-decoded and NFC-normalised | url, percent-encoding, unicode-normalization |
| `src/manifest.rs` | the manifest of a build, as serde types | serde, image (image headers), toml (base URLs) |
| `src/structdiff.rs`, `src/ratchet.rs` | the comparison, A7, the ratchet and its baselines | serde, similar (word hunks), globset (the changes files' key patterns) |
| `src/json.rs` | the JSON text of the harness's files: sorted keys, `, ` and `: `, one entry per line, gzip without name or time | serde_json, flate2 |
| `src/sites.rs` | the sites; `patches.json` as serde types | serde, filetime |
| `src/licence.rs`, `src/notices.rs`, `src/metadata.rs` | the licence check and the release notices; `cargo metadata`'s output as serde types (not the cargo_metadata crate, which turns on serde_json's `unbounded_depth`: `cargo dev` would build a second serde_json) | spdx (lax: `A/B` is `A OR B`), serde, semver |
| `src/package.rs` | the release archives | tar, flate2, zip, jiff |

What must agree with the golden data is what the comparison compares, not the bytes the tools
write: the four gates (A-T, A-D1, A-D2, A-D3) find no difference between the Rust builds and
the unchanged golden manifests, structure dumps and baselines. A difference's fingerprint is
the first 12 hex digits of the SHA-256 of the compared values' canonical JSON; the baselines
hold no difference, so none of the first implementation's fingerprints was kept.

## Tests

`cargo test -p ssg-dev`: the page and feed scans, the URL and path normalisation, txtar and the
JSON text (`tests/it/extract.rs`); structdiff's self-test, `patches.json` against the Tera
patch files, the changes files and the baselines (each written back as it was read), the sites
(`tests/it/harness.rs`); the SPDX expressions, `version.sh` (in a scratch git repository) and
the archives of `package` (`tests/it/release.rs`).
