# tools/rust-port

Site inputs for fugo's acceptance harness. The comparison itself is `tools/dev/compare.sh` with
the commands of ssg-dev (`crates/dev`: `sites`, `manifest`, `structdiff`; see
`docs/rust-port/HANDOFF.md`).

- `cargo dev sites` (`crates/dev/src/sites.rs`) generates every test site outside the repository
  (`make`, `cache`, `list`). `tools/dev/compare.sh` and the gate tests in
  `crates/cli/tests/it/` use it, as `tools/dev/oracle.sh` (at `44529028`) did for the golden
  data, with the site generator of that time (`tools/rust-port/i01/sites.py`).
- `i01/patches.json`: the edits of the legacy docs site per variant (i01, reduced, live), in
  the order they are applied. `cargo dev sites patches` rewrites it in its canonical form and
  checks it against the Tera patch files of `sites/docs/patches/` (a test of ssg-dev too).
- `i01/testsite.txtar`, `i01/errors.txtar`: inputs of the testsite and the error-text site
  (`cargo dev sites make`).
- `testdata/getremote-cache/docs-live/`: the GetRemote responses of the published docs build
  (its README.md lists them); `cargo dev sites cache docs-live` serves them, and `cargo dev
  sites cache mini` serves the mini site's entry from `testdata/oracle/commands/e2e/e2e.json.gz`.

**Golden data.** The reference of every comparison is what the Go version built, frozen since
the Go implementation was removed after commit `44529028`:

- `testdata/golden/`: the manifests and structure dumps of the testsite and the docs variants,
  and the golden images, written by `tools/dev/oracle.sh` (T01; the docs labels again in T65
  and T66); `testdata/golden/README.md` has the schema and the recipe to regenerate them in a
  worktree of `44529028`.
- `crates/build/tests/it/testsite-go.txtar`: the Go build of the testsite (gate A-T).

**Historical.** The byte-for-byte port's tools (`i01/compare.sh`, `i01/diff.py`, `compare.py`,
`build-site.sh`, `prepare-site.sh`) were removed.
