# AGENTS.md

Guidance for coding agents (and people) working on fugo. [CLAUDE.md](CLAUDE.md) points here.

- [DEVELOPMENT.md](DEVELOPMENT.md) is the developer's guide: the layout of the workspace, its
  rules for agents (worktrees, the shared `target/` directory, build commands, disk), the
  commands, CI and releases, and the test data.
- [CONTRIBUTING.md](CONTRIBUTING.md) is how to send a change.

## Files of at most 500 lines

No file of code may be longer than 500 lines: Rust (`*.rs`), TypeScript and JavaScript
(`*.ts`, `*.js`, `*.mjs`, `*.cjs`) and shell scripts (`*.sh`). The Lint job of CI runs the
check:

```sh
cargo dev file-length
```

Files that are code by their extension but not written by hand don't count: recorded fixtures
and third-party content (`testdata/`, `fixtures/`), built and vendored assets
(`crates/cms/assets/`, `crates/funcs/assets/katex/`) and generated files (the lexers and styles
converted from Chroma's XML, the es5 table of `crates/jsbuild`). `crates/dev/src/file_length.rs`
lists them, each with its reason; add to it only what no one edits by hand.

When a file would grow past 500 lines, split it by responsibility before adding to it, and name
each new module after what it holds, with a `//!` comment:

- Methods of a type: move a group of them to a child module, in its own `impl` block. A child
  module sees its parent's private items through `use super::*;`; a private method it moves
  that other modules call becomes `pub(super)`, and a `pub(super)` one `pub(in super::super)`
  (a wider visibility stays).
- Free functions and types: move a group to a child module, and re-export it from the parent
  (`use child::*;`, as visible as the widest item it holds) so that paths don't change.
- Tests (`crates/<name>/tests/it/<module>.rs`): move groups of tests to child modules,
  `tests/it/<module>/<topic>.rs`, declared in `tests/it/<module>.rs`, which keeps the helpers
  they share.
- Data that is not code (word lists, a script a crate embeds) can move to a file of its own,
  read with `include_str!`.
- TypeScript and JavaScript (`crates/cms/web`, `docs/assets/js`): split into modules with
  `import`/`export`, which `js_build` bundles; for the CMS, rebuild the embedded assets
  (`tools/cms/build.sh`).

Keep each file well under the limit, so that the next change doesn't need a split.
