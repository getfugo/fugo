# tools/rust-port

Site inputs for fugo's acceptance harness. The comparison itself lives in `tools/dev/`
(`compare.sh`, `structdiff.py`, `manifest.py`; see `docs/rust-port/HANDOFF.md`).

- `i01/sites.py` generates every test site outside the repository (`make`, `cache`, `list`) and
  keeps `i01/patches.json` and the Tera patch files of `sites/docs/patches/` in step
  (`patches [--check]`). `tools/dev/compare.sh` and the gate tests in
  `crates/cli/tests/it/` call it, as `tools/dev/oracle.sh` (at `44529028`) did for the golden data.
- `i01/testsite.txtar`, `i01/errors.txtar`: inputs of the testsite and the error-text site
  (`sites.py make`).
- `testdata/getremote-cache/docs-live/`: the GetRemote responses of the published docs build
  (its README.md lists them); `sites.py cache docs-live` serves them, and `sites.py cache mini`
  serves the mini site's entry from `testdata/oracle/commands/e2e/e2e.json.gz`.

**Golden data.** The reference of every comparison is what the Go version built, frozen since
the Go implementation was removed after commit `44529028`:

- `testdata/golden/`: the manifests and structure dumps of the testsite and the docs variants,
  and the golden images, written by `tools/dev/oracle.sh` (T01; the docs labels again in T65
  and T66); `testdata/golden/README.md` has the schema and the recipe to regenerate them in a
  worktree of `44529028`.
- `crates/build/tests/it/testsite-go.txtar`: the Go build of the testsite (gate A-T).

**Historical.** The byte-for-byte port's tools (`i01/compare.sh`, `i01/diff.py`, `compare.py`,
`build-site.sh`, `prepare-site.sh`) were removed.
