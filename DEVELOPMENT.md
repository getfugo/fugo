# Developing fugo

fugo is the Cargo workspace at the repository root: an idiomatic Rust rewrite of the Go implementation's
site and page model, with Tera 2 templates. The plan, binding for every task and review, is
[`docs/rust-port/REWRITE_PLAN.md`](docs/rust-port/REWRITE_PLAN.md); §1.2 "What Rust style means
here" is the review checklist. Its paths below `rust/` and the binary name it uses predate
the move to the root and the drop-in names (HANDOFF §9): read `rust/<path>` as `<path>`
(`rust/README.md` is this file, `rust/docs/template-api.md` is
`docs/rust-port/template-api.md`) and its binary name as `fugo`; where the plan differs from
this file or HANDOFF on paths and names, they win. The current state (crate map, commands,
gates, deviations, open items) is [`docs/rust-port/HANDOFF.md`](docs/rust-port/HANDOFF.md).

**The old port** (the byte-identical tree in the root `crates/` of `be02933a`, not today's
crates, and its byte-exact Go oracles) was deleted in T00. It is recoverable at that commit,
tagged `go-parity-final` in the local repository (the tag is not on GitHub):
`git show go-parity-final:crates/<crate>/<path>`. Salvage rules, not code, from it (§6.2).

**The Go implementation** (the Go tree, `tools/go-oracle`, `tools/dev/oracle.sh` and the
Go workflows) was removed after commit `44529028`. What it generated is frozen:
`testdata/oracle/`, `testdata/golden/`, `crates/build/tests/it/testsite-go.txtar`,
`crates/highlight/tests/data/` with `crates/highlight/src/data/chroma-lexers.tsv`,
`crates/funcs/tests/fixtures/remarshal/go.txt` and `testdata/legacy-docs/data/docs.yaml`, as well as the Go
outputs the old port recorded at `be02933a`, such as `testdata/corpus/minify/*.tsv`
(PROVENANCE.md). To regenerate the data of `44529028`, run the old recipe in a worktree of it
(`git worktree add <dir> 44529028`) and copy the result back: `testdata/golden/README.md`,
`crates/highlight/README.md`, the docstring of `tools/dev/fixtures2json.py`; `docs.yaml` is
written by the Go binary's `gen docshelper`.
The Go tree's test data that the tests read is in `testdata/upstream/`, at its Go-tree path. Go-tree
paths in comments and READMEs (`resources/images/text.go`, `tpl/tplimpl/embedded/templates/`, …)
name files of that commit: `git show 44529028:<path>`.

**The workspace moved to the repository root** after the Go implementation was removed; until
then it was `rust/` (at `44529028` as well: `git show 44529028:rust/<path>`). Ids recorded
before the move keep the prefix (the sources of `testdata/golden/images/manifest.json`, e.g.
`rust/testdata/site-assets/…`); `ssg_testkit::fixture::repo_file` and `sites.py`'s
`repo_file` resolve them at the root. The move commit (`7e58cfce`) also rewrote nearly every
line of `PROVENANCE.md`, so git does not see that rename: its history before the move is
`git log -- rust/PROVENANCE.md`.

## Layout

```
Cargo.toml  Cargo.lock      all third-party deps and features live in [workspace.dependencies]
clippy.toml deny.toml       thread_local ban; licence policy
PROVENANCE.md THIRD_PARTY/  every non-original file; licences cargo cannot see
crates/<name>/              the product crates of §2.1 (T00 wrote stubs with the real
                            dependency edges of §2.3), testkit (dev), workspace-hack (internal)
testdata/oracle/<area>/     Go-oracle fixtures as plain JSON (fugo schema, below; frozen)
testdata/golden/<label>/    the Go build's manifests, structure dumps and images (frozen)
testdata/baselines/         the ratchet's baselines (tools/dev/changes/README.md)
testdata/corpus/            corpora: date formats, Thai strings
testdata/site-assets/       the images tools/rust-port/i01/sites.py puts into its sites
testdata/upstream/          the Go tree's test data the tests read, at its path (fixture ids);
                            goroot/: Go's image test data the image oracles read
                            old-port/: five more of them, from be02933a
testdata/COUNTS.json        per fixture: old path, record and value counts at conversion
sites/<site>/               the Tera layouts (and assets) of the test sites; sites/docs/patches/
tools/dev/              the harness (compare.sh, structdiff.py, manifest.py, selftest.py,
                            changes/), node.sh, licence-check.sh, notices.py, package.py, disk.sh,
                            fixtures2json.py
tools/rust-port/            i01/sites.py (every test site), patches.json and the site txtars; the
                            docs-live GetRemote cache (its README.md)
tools/cms/                  build.sh: the CMS editor (crates/cms/web, TypeScript and Sass) into crates/cms/assets
docs/                       fugo's documentation site (tools/docs/build.sh); docs/rust-port/: the
                            plan, the handoff, template-api.md
testdata/legacy-docs/       the Go build's documentation site, a test site (frozen)
.github/workflows/ci.yml    CI and releases (below)
```

Member crates: `[lib] doctest = false`; one integration binary `tests/it/main.rs`
(`autotests = false`); `view`, `sitefuncs`, `render`, `build` and `cli` also set
`[lib] test = false`. Members never add third-party `features =`; every member depends on
`ssg-workspace-hack`. Each stub lists its *planned* third-party dependencies so that
`Cargo.lock` pins them; the owning task adjusts the list.

## Rules for agents (§8.1)

- **One git worktree per agent**, based on `rust-port`. Only green commits are merged; rebase
  before merging. At most one agent edits a given crate at a time (crate lock); a bug found in
  another crate becomes a short fix task that takes that crate's lock.
- **Environment** (the agents' disk budget, §2.2; the repository has no `.cargo/config.toml`, so
  a plain `cargo build` builds into `target/` with Cargo's defaults):
  ```sh
  export CARGO_TARGET_DIR=<main checkout>/target   # shared by all worktrees
  export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
  ```
- **Build commands:** only `cargo test -p ssg-<crate>` (`crates/cli` is package `ssg-cli`;
  plus `-p` of direct dependants after an API change). Never `cargo check`, `clippy` or `doc` in the edit–test loop; never `--workspace`;
  `--release` only for measurements and packaging (since T70); never `cargo clean` (`cargo clean -p X` only when coordinated).
  `cargo fetch`/`cargo metadata` use `--target x86_64-unknown-linux-gnu` /
  `--filter-platform x86_64-unknown-linux-gnu`. Builds run offline after T00.
- **Shared target dir and stale artifacts.** Every worktree has the same layout, so a path crate
  gets the same cargo metadata hash in every worktree and cargo may reuse another worktree's
  artifact when its source mtimes look older. Before a final test run (and when a result looks
  impossible), `touch` the sources of the crates you test, e.g.
  `find crates/<name> -name '*.rs' -exec touch {} +`. Read data paths at run time with
  `ssg_testkit::fixture::{repo_dir, testdata}`, never with `env!("CARGO_MANIFEST_DIR")`.
- **Disk:** `tools/dev/disk.sh` fails above 8 GB of `target` (`FUGO_TARGET_LIMIT_MB`) or below 2 GB free (raised from the plan's 2.5 GB once ~20 GB became free).
  Every task reports `du -sh "$CARGO_TARGET_DIR"` and `df -h /` when it ends.
- **Clean room:** never open Zola ≥ 0.22 source (EUPL-1.2). Copied files go through
  `PROVENANCE.md` first.
- **Review checklist:** §1.2 (typed model, no thread-locals, no Go-order emulation, no Go error
  texts, no Go method tables or transliterated state machines, no Zola-isms) plus the task's
  acceptance criteria.

## Lanes

Four lanes (§8.3): A the model and critical path (T00 → T10 → T13 → T20 → T21 → T23a → T23b →
T33 → T34 → T36 → T37 → T60 → T65); B templates, markup and render; C harness, output and
resources; D layout conversion, images and nav. Heavy crates (`highlight`, `minify`, `images`,
ICU data in `locale`, `serve`) stay out of lanes A/B until round 8.

## Commands

| What | Command |
|---|---|
| per crate | `cargo test -p ssg-<crate>` (`crates/cli` is package `ssg-cli`, binary `fugo`; from your worktree's root) |
| phase end | `cargo clippy -p ssg-<crate> -- -D warnings` per crate of the phase; `cargo fmt --check`; `tools/dev/licence-check.sh` |
| graph | `cargo metadata --format-version 1 --filter-platform x86_64-unknown-linux-gnu` |
| disk | `tools/dev/disk.sh` |
| fixtures | `tools/dev/fixtures2json.py convert <dir> <dir>` after regenerating a Go oracle (in a worktree of `44529028`) |
| acceptance | `tools/dev/compare.sh <site> [--docs-patches i01\|reduced\|live] [KEEP=1]` (T03) |
| docs site | `tools/docs/build.sh [-o <dir>] [--serve]`: fugo's documentation, `docs/` (its own Tera theme; no node tools); `cargo test -p ssg-cli docs_site` builds it and fails on any warning (broken links included). The reference data in `docs/data/` is generated: `INSTA_UPDATE=always cargo test -p ssg-testkit contract` (`template_api.json`) and `-p ssg-cli docs_data` (`commands.json`) |
| legacy docs | `tools/legacy-docs/build.sh [-o <dir>] [--serve]`: the Go build's documentation site (`testdata/legacy-docs` + the Tera overlay `sites/docs`, its own node modules), as the Go build's website published it; gate A-D3 compares it with the published site |
| templates | `fugo templates check -s <site-dir>` (T37) |
| CI, locally | see "CI and releases" below (workspace-wide: not for the edit–test loop) |

## CI and releases

`.github/workflows/ci.yml` builds, tests and releases this workspace; it is the repository's
only build workflow. `bump.yml` cuts a release, `image.yml` packages each release as a container
image (below) and `stale.yml` manages issues.

**When it runs.** On pushes to `main` and `rust-port`, on every pull request, on every
`v[0-9]*` tag, and by hand (`workflow_dispatch`); there are no path filters. A newer run on the
same ref cancels the older one, except on tags.

| Job | Runner | Steps |
|---|---|---|
| Lint | ubuntu-24.04 | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `tools/dev/licence-check.sh`; `tools/dev/selftest.py`; `tools/rust-port/i01/sites.py patches --check`; on a tag, the tag must be `v<version of [workspace.package]>` |
| Test | ubuntu-24.04 | `cargo test --workspace --locked --no-fail-fast` with the tools below; the job summary lists every test that printed `SKIPPED`, and any such test fails the job |
| Build | one native runner per target | `cargo build --release --locked -p ssg-cli --target <triple>` with the version variables (below); `fugo version`; a tiny site built, whose sitemap, RSS and `robots.txt` (embedded templates) must have no CR; `tools/dev/notices.py` (licences of the linked crates); `tools/dev/package.py` → artifact `fugo-<triple>` |
| Release | ubuntu-24.04 | tags only, after the other three: the GitHub release (below), then `image.yml` for its container image |

The tests run on Linux only; the macOS and Windows binaries get the release build and its smoke
test (the version line and one tiny site). The Go CI also ran its tests on Windows (`mage -v
test` on `windows-latest`; `.github/workflows/ci.yml` at `44529028`). A Windows leg of the Test
job is an open item (HANDOFF §8): the test crates do not build there yet, since
`crates/publish/tests/it/staticcopy.rs` and the fake tools of
`crates/resources/tests/it/pipes/` use `std::os::unix` without a `cfg(unix)` gate.

Build targets and runners: `x86_64-unknown-linux-gnu` (ubuntu-22.04), `aarch64-unknown-linux-gnu`
(ubuntu-22.04-arm), `x86_64-apple-darwin` (macos-15-intel), `aarch64-apple-darwin` (macos-15),
`x86_64-pc-windows-msvc` (windows-2025). The binary's native C code, libwebp (`webp` →
`libwebp-sys`) and `ring` (`rustls`, for `get_remote`), compiles with each runner's own C
compiler through the `cc` crate; there are no cross toolchains. The Linux binaries are linked on
Ubuntu 22.04, so they need no glibc newer than its 2.35 (the smoke test prints the exact
version; linked on 24.04 they would need 2.39: std's weak `pidfd_spawnp` reference still
records `GLIBC_2.39`). The Windows binary links the MSVC runtime statically (`+crt-static`), so
it needs no Visual C++ redistributable. Text files are checked out with LF on every runner
(`.gitattributes`: `* text=auto eol=lf`), so the templates the binary embeds and the files the
archives ship have LF on Windows too, as the Go releases (cross-built on Linux) had.

The release is a drop-in replacement for the Go releases (`.goreleaser.yml` at `44529028`):
the binary is `fugo` (`fugo.exe`), and an archive is named as theirs,
`fugo_<version>_<os>-<arch>.tar.gz` (`.zip` for Windows; `<os>` `linux`, `darwin` or
`windows`, `<arch>` `amd64` or `arm64`). It holds, at its root, the binary, `README.md` and
`LICENSE` (as the Go archives did) and `THIRD_PARTY_NOTICES.txt`, `PROVENANCE.md` and
`THIRD_PARTY/`; the build job writes a `.sha256` next to it. `fugo version` prints the Go
format, `fugo v<version>[-<commit>] <os>/<arch> BuildDate=<date|unknown>[ VendorInfo=<vendor>]`
(`crates/cli/src/version.rs`): the build job sets `FUGO_BUILD_COMMIT` (the commit),
`FUGO_BUILD_DATE` (its time in UTC, RFC 3339) and `FUGO_VENDOR_INFO=fugo` for the
build, and the smoke test checks the line; without them (a local build) the line has no commit,
`BuildDate=unknown` and no vendor. `package.py` takes the version from `Cargo.toml` and checks
that `fugo version` prints exactly `v<version>`, followed by `-$FUGO_BUILD_COMMIT` when
that is set (else by a hex commit or nothing).

`THIRD_PARTY_NOTICES.txt` is written by `tools/dev/notices.py <triple> <file>`: the licence
and notice files of every crate linked into `fugo` for that target (the normal-dependency
closure of `fugo` without proc macros; vendored C libraries such as libwebp included), each
text printed once. A linked crate that ships no licence file gets the MIT text when MIT is one of
its licences, or a reference to the Apache-2.0 text another linked crate ships when Apache-2.0
is; any other such crate fails the step, so it is noticed before a release.

**Toolchain.** CI installs `RUST_TOOLCHAIN` (1.96.0, the toolchain the workspace is developed
with) through rustup; `rust-version = "1.96"` in `Cargo.toml` stays the MSRV, set by rolldown's
oxc (1.96 for oxc 0.152). rolldown and the oxc it pins are exact pins (`=`): their Rust API has
no semver promise and each release moves oxc, which in turn raises its MSRV every few
releases, so a rolldown upgrade is its own change that also moves `rust-version` and
`RUST_TOOLCHAIN` when needed. A newer clippy brings new lints, which `-D warnings` turns into
failures, so the pin moves in a commit of its own that also fixes the new findings. There is
deliberately no `rust-toolchain.toml`: a pinned channel makes rustup look for a toolchain named
after it and try to download it in environments that cannot. Add one (and drop
`RUST_TOOLCHAIN`) once every environment can install toolchains.

**Tools the tests use.** Without its tool a test prints `SKIPPED …` and passes, so CI provides
all of them, and its Test job fails when a test prints `SKIPPED`:

| Tests | Tool | In CI |
|---|---|---|
| `ssg-jsbuild`: `jsbuild_synth`, `jsbuild_docs` and the `build::` tests that run scripts (the oracle's and fugo's bundles run side by side, compared by what they do) | `node` on `PATH` | `actions/setup-node`, Node 22 |
| `fugo`: `gate_a_d2`, `gate_a_d3` (`tools/dev/compare.sh … --ref golden`) | `python3` and `bash` on `PATH`; the node modules of `tools/dev/node.sh` (Alpine.js, Turbo), which compare.sh links into each site for `js_build` | the runner's `python3` and `bash`; `tools/dev/node.sh` |

`ssg-npm`'s tests and `fugo`'s `npm::` tests need no tool and no network: `ssg_testkit::registry`
serves their packages on `127.0.0.1`.

`ssg-images`' `sizes_match_the_process_oracle` compares all 12,264 cases, 2,204 of them from
Go's own image test data in `testdata/upstream/goroot/` and `testdata/upstream/old-port/`
(`crates/images/README.md`); a source that cannot be found fails it. Python
is needed only by the gate tests and the tools (`licence-check.sh`, `selftest.py`, `notices.py`,
`package.py` (Python 3.11 or later), and `tools/rust-port/i01/sites.py` for the ignored
real-site tests that read `FUGO_SITES`). Not needed by `cargo test`: dart-sass (grass
compiles Sass in process) and the network (`get_remote` tests read caches with the network off).

**Reproduce CI locally** (from the repository root, offline; `touch` the sources first, see the
shared-target note above):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
tools/dev/licence-check.sh
python3 tools/dev/selftest.py
python3 tools/rust-port/i01/sites.py patches --check
tools/dev/node.sh check       # once: tools/dev/node.sh
cargo test --workspace --locked --offline --no-fail-fast -- --show-output
cargo build --release --locked --offline -p ssg-cli --target x86_64-unknown-linux-gnu \
  --target-dir <scratch>/target                  # never the shared target dir
python3 tools/dev/notices.py x86_64-unknown-linux-gnu <scratch>/THIRD_PARTY_NOTICES.txt
python3 tools/dev/package.py <scratch>/target/x86_64-unknown-linux-gnu/release/fugo \
  x86_64-unknown-linux-gnu <scratch>/dist <scratch>/THIRD_PARTY_NOTICES.txt
```

**Cutting a release.** The version comes from the git tag: a release is the tag
`v<major>.<minor>.<patch>[-<pre-release>]`, and nothing in the repository is edited for it.
`version` in `[workspace.package]` of `Cargo.toml` (`0.0.0-DEV`) is only the version of builds
not made from a tag. fugo's releases start at `v1.0.0`. The Go fork's tags (`v0.1.0` …
`v0.148.2`) and their releases were removed when `v1.0.0` was released; a v0.x tag (an old
clone may still have them) never counts.
1. Run the **Bump version** workflow (`.github/workflows/bump.yml`; Actions → Bump version →
   Run workflow, or `gh workflow run bump.yml --ref main -f version_bump=minor`) on the branch
   to release, with `version_bump` `major`, `minor` or `patch`. It takes the latest release tag
   (`tools/dev/version.py next`; pre-releases do not count), bumps that part (`v1.4.2` → major
   `v2.0.0`, minor `v1.5.0`, patch `v1.4.3`; `v1.0.0` when there is no release yet), tags the
   branch head and starts CI on the tag. A tag pushed with the workflow's token starts no
   workflow by itself, so it dispatches `ci.yml` with the tag as ref. The workflow must be on
   the default branch to appear in the Actions tab. A tag pushed by hand (`git tag v1.2.3
   <commit>`, `git push origin v1.2.3`; also for a pre-release such as `v1.3.0-rc.1`) starts CI
   the same way.
2. CI on the tag: Lint checks that the tag is a release tag (`tools/dev/version.py tag`), the
   five builds compile with `FUGO_BUILD_VERSION=<version>` (read at compile time by
   `ssg_base::VERSION`: `fugo version`, `build.version`, the generator tag) and `package.py`
   names the archives after it. After lint, test and the builds pass, the Release job creates
   the GitHub release `v<version>`, titled `fugo <version>`, with the five archives and
   `fugo_<version>_checksums.txt` (`<sha256>  <archive>` per archive, the checksums file of the
   Go releases; the `.sha256` files of the build jobs are checked and joined into it). A
   version with a `-` (e.g. `1.3.0-rc.1`) makes a pre-release, never latest; any other release
   is marked latest only if no published release has a higher version, so a patch release of
   an older line does not take latest from a newer one.
3. The Release job then dispatches `image.yml` with the tag as ref, which pushes the release's
   container image (below).

A local build gets a release's version the same way: `FUGO_BUILD_VERSION=1.2.3 cargo build
--release --locked -p ssg-cli`.

Re-running the workflow (or its failed jobs) for a tag replaces the assets of the release an
earlier run created, and publishes it if that run stopped before (`gh release create` makes a
draft, uploads the assets, then publishes it).

**Container image.** `Dockerfile` packages a published release, it does not compile:
`docker build --build-arg FUGO_VERSION=<version> .` downloads
`fugo_<version>_linux-<amd64|arm64>.tar.gz` and the checksums file of the release `v<version>`
(on the build machine's platform, so nothing is emulated), checks the archive, and puts the
binary in `/usr/local/bin` of `debian:trixie-slim` (glibc 2.41; the binaries need 2.35) with
ca-certificates, git and tzdata (jiff reads time zones from `/usr/share/zoneinfo` on Linux), and
the archive's licence files in `/usr/share/doc/fugo/`. A `RUN fugo version` step checks that the
binary runs on the base and prints the version asked for. The image runs as the user `fugo`
(1000:1000) in `/src` with the entry point `fugo`; `XDG_CACHE_HOME=/cache` (mode 1777) and git's
`safe.directory = *` let any `--user` build a mounted site. `.dockerignore` excludes the whole
build context, which the Dockerfile does not use. `FUGO_RELEASES` (a build argument) points it at
another copy of the releases, e.g. a local `python3 -m http.server` serving
`v<version>/<files>` of a CI run's artifacts to test the Dockerfile before a release
(`--build-arg FUGO_RELEASES=http://host.docker.internal:8000`).

`.github/workflows/image.yml` builds it for linux/amd64 and linux/arm64 (QEMU runs the final
stage's `apt-get` for arm64) and pushes `ghcr.io/getfugo/fugo` with the job's token. It runs by
`workflow_dispatch` with a release tag: the Release job dispatches it for each release, and
`gh workflow run image.yml -f tag=v1.2.3` builds a published release's image again (from the
default branch's Dockerfile, unless `--ref` names another). It pushes the tags `<version>` and,
when no published release of that line has a higher version, `<major>.<minor>`, `<major>` and
`latest` (the Release job's rule for latest); a pre-release gets only `<version>`. Before
pushing, it runs the amd64 image on a tiny site, as the runner's user through a mounted
directory, and checks the sitemap's time zone. A pull request that changes `Dockerfile`,
`.dockerignore` or `image.yml` builds the latest release's image without pushing. A new package
on ghcr.io is private: make it public once in its settings (Package settings → Change
visibility).

**Caches.** `Swatinem/rust-cache` per job and target; only pushes to `main` and `rust-port` save
them, and pull requests restore their base branch's. A cold test job builds about 3.2 GB of
`target/` (1.8 GB of it test executables, 0.8 GB of which debug info: `debug =
"line-tables-only"` of the dev profile already applies), well within the runner's disk.

## Test data: the fugo schema

Fixtures are JSON documents (`.json`) or JSON lines (`.jsonl`), gzipped when named `.gz`.
The Go oracles' type tags are unwrapped into plain JSON; Go values plain JSON cannot hold are
single-key objects: `{"$nh:time": "<RFC 9557>"}`, `{"$nh:local": "<TOML local date/time>"}`,
`{"$nh:bytes": "<hex>"}` (not UTF-8), `{"$nh:float": "NaN"|"+Inf"|"-Inf"}`,
`{"$nh:keys": [..]}`, `{"$nh:len": n}`. Other objects with a `"t"` key are oracle records.
The full description is the docstring of `tools/dev/fixtures2json.py`; read fixtures with
`ssg_testkit::fixture` (`oracle`, `oracle_lines`, `Tag`, `GoString`) and compare semantic
fields only, with a reviewed `expected_diffs.toml` per crate.

## Optional features

`ssg-funcs` compiles two SHOULD template functions only with a feature: `to_math` (`math`,
KaTeX 0.16.22 with mhchem run in QuickJS-ng through rquickjs, as the Go implementation runs it;
`crates/funcs/README.md`) and `diagrams_goat` (`goat`, the port of GoAT in `src/pure/goat/`).
Without it the name is registered as a stub that fails when called. The `fugo` crate (the
`fugo` binary) turns both on by default (`default = ["goat", "math"]`), so the release build and
CI's `-p ssg-cli` builds render the documentation site's formulas and diagrams (gate A-D2, T66);
`--no-default-features` leaves them out. Their cost: `goat` has no dependencies (the GoAT port
replaced svgbob and its geometry crates, nalgebra, parry2d, sauron, futures, …); `math` brings
rquickjs, rquickjs-core and rquickjs-sys (MIT), whose build script compiles QuickJS-ng's C
sources with the platform's compiler (`cc`, as libwebp-sys does; rquickjs-sys ships bindings for
the five release targets, so no bindgen or libclang), and about 1.45 MB (2.9 %) of the stripped
release binary (QuickJS and the 310 KB of KaTeX JavaScript). A test of `ssg-funcs` alone
builds without them unless `--features goat,math` is given (`tests/it/{math,goat}.rs`).

`ssg-cli` also turns on `npm` by default (`crates/npm/README.md`): `build` and `server` install
the project's `package.json` with Deno's npm installer, without Node.js. Nothing runs the
packages' programs (fugo has no JavaScript runtime besides `math`'s QuickJS). Its cost: 78 more
crates in the binary's graph (636 instead of 558: Deno's installer crates, pinned exactly as one
release, and their dependencies) and about 6 MB (9 %) of the stripped release binary (71.0 MB
instead of 64.8 MB, macOS arm64). `cargo test -p ssg-cli --no-default-features --features
goat,math` builds without it; that binary installs nothing, and a site installs its
`node_modules` itself.

## Feature unification

Cargo unifies features per invocation over the selected packages, so `-p a` and `-p b` could
build a shared dependency twice with different features. cargo 1.94.1 has no stable
`resolver.feature-unification = "workspace"` (it warns "ignoring `resolver.feature-unification`
without `-Zfeature-unification`"), so `crates/workspace-hack` depends on the light shared
dependencies with the union of their features (declared in the "workspace-hack only" block of
`Cargo.toml`). Check it after changing dependencies: for every member `m`, run

```sh
cargo tree -p m --target x86_64-unknown-linux-gnu -e normal,dev,no-proc-macro \
  --prefix none --no-dedupe -f '{p}|{f}'
```

and compare each package's feature list across members; a light crate that differs goes into
the hack with the union. Deliberately left out (heavy, or used by one lane only): ICU crates with
compiled data (`icu_normalizer`, `icu_properties`, `icu_calendar`, `icu_time`), `phf`, `rand`,
`getrandom` 0.2/0.4, `hashbrown` 0.14, `http`, `httparse`, `tracing-core`, and host-only
proc-macro dependencies (`syn`).

**serde_json.** rolldown and oxc_resolver turn on serde_json's `preserve_order` (a `Map` keeps
insertion order; package.json `exports` depend on it) and rolldown_common its
`arbitrary_precision` (numbers keep their text), in every build `ssg-jsbuild` is part of —
the binary included. The hack turns both on for every member, so every test runs with what the
binary does (`ssg-funcs`' `determinism` test fails when they are off). Two rules follow:
never rely on the order of a `serde_json::Map` (convert to `ssg_base::Value`, whose maps
are sorted, before output; tests compare JSON as values or through that conversion), and never
hand a `serde_json::Value` with numbers to another format's serializer (Tera, YAML, TOML): its
numbers serialize as a private one-entry map. `ssg_base::Value`'s deserializer reads such
a map back as the number, and `ssg_testkit::tera_value` converts JSON for template tests.
