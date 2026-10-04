---
title: Development
description: Build fugo from source, find your way around the Cargo workspace, and run its tests.
weight: 10
---

## Build

You need Rust 1.96 or later and a C compiler:

```sh
git clone https://github.com/getfugo/fugo.git
cd fugo
cargo build --release --locked -p ssg-cli
target/release/fugo version
```

## The workspace

fugo is a Cargo workspace. Each crate under `crates/` is a package named `ssg-<crate>`; the
binary `fugo` is the package `ssg-cli`.

| Crate | What it does |
|---|---|
| `cli` | the command line, `templates check`, the server's front end |
| `build` | the build: phases, waves, deferred work |
| `config` | configuration loading, merging, themes |
| `vfs` | the union file system of mounts and themes |
| `pageparser`, `page`, `site` | content files, pages, the site model |
| `markup` | Markdown rendering (comrak with goldmark's behaviour) |
| `highlight` | the Chroma port |
| `layouts`, `render`, `view` | template lookup, Tera rendering, the values templates see |
| `funcs`, `sitefuncs` | template functions (`spec.rs` declares every one) |
| `resources`, `images`, `jsbuild`, `minify` | asset pipelines |
| `npm` | installing `package.json` (Deno's npm installer) |
| `publish`, `serve` | writing files, the development server |
| `cms` | the [browser editor](/content-management/cms/): its settings, content index and API Worker |
| `locale`, `nav`, `base` | languages, menus and pagination, shared types |
| `testkit` | fixtures, the contract tests, a local npm registry |

`docs/rust-port/HANDOFF.md` is the map of the code, the parity gates and the known
deviations; each crate's README describes its API, its tests and its accepted differences.

## Tests

```sh
cargo test -p ssg-markup             # one crate
cargo test --workspace               # everything (slow)
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Many tests compare fugo with the Go implementation's output, recorded once from its Go code and
kept in `testdata/`: its documentation site (`testdata/legacy-docs/`), test sites, and the
outputs of its functions. A change that alters output must say why, in the crate's
`expected_diffs.toml` or README.

## The browser editor's scripts

The editor and its API Worker are TypeScript and Sass in `crates/cms/web/`. The crate embeds
what they build to (`crates/cms/assets/admin/cms.js`, `cms.css`, `assets/worker.js`), so after
changing them, rebuild those files with fugo itself:

```sh
tools/dev/node.sh                     # once: the libraries the editor bundles, and tsc
cargo build --release -p ssg-cli
tools/cms/build.sh                    # type-check (strict), then build into crates/cms/assets
cargo test -p ssg-cms                 # the editor's and the Worker's tests (node --test)
```

A test of `ssg-cli` fails when the embedded files are not what the sources build to.

## Adding a template function

1. Declare it in `crates/funcs/src/spec.rs`: name, kind, arguments, phase, documentation and
   the Go-template names it replaces.
2. Implement it in `ssg-funcs` (pure) or `ssg-sitefuncs` (reads the site).
3. Regenerate the reference data:
   `INSTA_UPDATE=always cargo test -p ssg-testkit contract`. This updates
   `docs/data/template_api.json`, from which this documentation's
   [function reference](/reference/functions/) is built.
