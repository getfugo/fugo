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

The module documentation has the details (`src/sites.rs`, `src/manifest.rs`, `src/structdiff.rs`,
`src/selftest.rs`, `src/licence.rs`, `src/notices.rs`, `src/package.rs`). Release versions are
`tools/dev/version.sh` (POSIX sh: `bump.yml` and `image.yml` run it without a Rust toolchain).

## The first implementation

Until v1.0.0 these tools were Python scripts (`tools/dev/*.py`, `tools/rust-port/i01/sites.py`;
`git show v1.0.0:<path>`). The golden manifests of `testdata/golden/` and the diff fingerprints
of `testdata/baselines/` were written by them, so this port reproduces their output byte for
byte: the HTML tokenizer is CPython's `html.parser` (`src/html.rs`), the URL functions are
`urllib.parse` (`src/url.rs`), the word hunks are `difflib` (`src/difflib.rs`), the change
patterns are `fnmatch` (`src/fnmatch.rs`), and JSON, `repr`, floats and whitespace follow
Python's rules (`src/py.rs`); PROVENANCE.md and `THIRD_PARTY/cpython/`.

When it was ported, both implementations gave the same bytes for:

- the manifests (all levels, with the full text) of the testsite and of docs-i01, docs-reduced
  and docs-live, both passes; of five generated sites of 1,500 pages each, with feeds,
  sitemaps, JSON and image files full of edge cases (malformed tags and attributes, character
  references, raw-text elements, URLs), and 3,000 JSON files, valid and invalid; and of 2,499
  real docs pages mutated at random;
- the reports, JSON results and written baselines of structdiff over 13 comparisons with and
  without differences, baselines, changes files and `--update`;
- every site of `sites list`, with and without the overlay (paths, bytes, modes, file times), and
  the caches;
- the licence check, the notices of four release targets, the archives' entries, and the
  self-test's output.

`tests/data/reference.json` keeps the values of the first implementation for synthetic inputs;
`tests/it/reference.rs` checks the port against them.

## Tests

`cargo test -p ssg-dev`: the reference values, structdiff's self-test, `patches.json` against
the Tera patch files, the changes files and the baselines, the sites, the SPDX expressions,
`version.sh` (in a scratch git repository) and the archives of `package`.
