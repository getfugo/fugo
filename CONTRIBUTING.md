# Contributing to fugo

We welcome contributions to fugo of any kind, including documentation, bug reports,
issues, feature requests, feature implementations, pull requests, answering questions in the
discussions, helping to manage issues, etc.

fugo began as a fork of another static site generator and was rewritten in Rust; see
[NOTICE](NOTICE) for the attribution of the derived material ([README](README.md#origin-and-attribution)).
GitHub's guide to [contributing to a project](https://docs.github.com/en/get-started/exploring-projects-on-github/contributing-to-a-project)
helps if you're unfamiliar with GitHub or contributing to open source projects in general.

*`docs/` is fugo's documentation site ([getfugo.github.io](https://getfugo.github.io)), built with fugo; `docs/rust-port/` holds the notes of the Rust rewrite. `testdata/legacy-docs/` is a frozen copy of the Go build's documentation site that the tests read byte for byte: do not edit it.*

*Changes to the codebase **and** related documentation, e.g. for a new feature, should still use a single pull request.*

## Table of Contents

* [Asking Support Questions](#asking-support-questions)
* [Reporting Issues](#reporting-issues)
* [Submitting Patches](#submitting-patches)
  * [Code Contribution Guidelines](#code-contribution-guidelines)
  * [Git Commit Message Guidelines](#git-commit-message-guidelines)
  * [Fetching the Sources From GitHub](#fetching-the-sources-from-github)
  * [Building and Testing Your Changes](#building-and-testing-your-changes)

## Asking Support Questions

Ask questions about fugo in the [discussions](https://github.com/getfugo/fugo/discussions)
of this repository.
Please don't use the GitHub issue tracker to ask questions.

## Reporting Issues

If you believe you have found a defect in fugo, use the
[issue tracker](https://github.com/getfugo/fugo/issues) of this repository to report
the problem. If you're not sure if it's a bug or not,
start by asking in the [discussion forum](https://github.com/getfugo/fugo/discussions).
When reporting the issue, please provide the version of fugo in use (`fugo
version`), your operating system, and, for a site converted from Go templates, whether a
Go-template generator builds it differently. Report only fugo issues here.

## Code Contribution

fugo is a fully featured static site generator, so any new functionality must:

* be useful to many.
* fit naturally into what fugo does: build sites from Markdown content and Tera templates fast.
* strive not to break existing sites.
* close or update an open [issue](https://github.com/getfugo/fugo/issues)

If it is of some complexity, the contributor is expected to maintain and support the new feature in the future (answer questions in the discussions, fix any bugs etc.).

Any non-trivial code change needs to update an open [issue](https://github.com/getfugo/fugo/issues). A non-trivial code change without an issue reference with one of the labels `type: bug` or `type: feature` (the labels the issue templates apply) will not be merged.

A new third-party crate goes into `[workspace.dependencies]` of `Cargo.toml` (members
never add their own `features =`) and must pass `tools/dev/licence-check.sh`, which allows
only the licences of `deny.toml`. Code or data taken from another project needs a row in
`PROVENANCE.md` first (and its licence in `THIRD_PARTY/` when cargo cannot see it);
Zola 0.22 and later (EUPL-1.2) must not be opened or copied.

**Bug fixes are, of course, always welcome.**

## Submitting Patches

The fugo project welcomes all contributors and contributions regardless of skill or experience level. If you are interested in helping with the project, we will help you with your contribution.

### Code Contribution Guidelines

Because we want to create the best possible product for our users and the best contribution experience for our developers, we have a set of guidelines which ensure that all contributions are acceptable. The guidelines are not intended as a filter or barrier to participation. If you are unfamiliar with the contribution process, the fugo maintainers will help you and teach you how to bring your contribution in accordance with the guidelines.

To make the contribution process as seamless as possible, we ask for the following:

* Go ahead and fork the project and make your changes.  We encourage pull requests to allow for review and discussion of code changes.
* When you’re ready to create a pull request, be sure to:
    * Make sure you may contribute the code under the [Apache License 2.0](LICENSE): by submitting a pull request you license your contribution under it (section 5 of the licence). There is no separate contributor licence agreement. Code or data copied from another project needs its `PROVENANCE.md` row (above).
    * Have test cases for the new code. If you have questions about how to do this, please ask in your pull request.
    * Run `cargo fmt --all`.
    * Add documentation if you are adding new features or changing functionality: the crate's `README.md`, and for template functions their entry in `crates/funcs/src/spec.rs`, which generates `docs/rust-port/template-api.md` and the function reference of the documentation site (`docs/`). Leave `testdata/legacy-docs/` unchanged: the tests record its files by hash.
    * Record any change of a parity difference (a test site's output against the Go implementation's) in the ratchet: the baselines in `testdata/baselines/` change only through an entry in `tools/dev/changes/<task>.md` (`tools/dev/changes/README.md`; `DEVELOPMENT.md`).
    * Squash your commits into a single commit. `git rebase -i`. It’s okay to force update your pull request with `git push -f`.
    * Ensure that the checks under [Building and Testing Your Changes](#building-and-testing-your-changes) succeed. The CI workflow (`.github/workflows/ci.yml`) runs them on every pull request and fails the build if one fails.
    * Follow the **Git Commit Message Guidelines** below.

### Git Commit Message Guidelines

This [blog article](https://cbea.ms/git-commit/) is a good resource for learning how to write good commit messages,
the most important part being that each commit message should have a title/subject in imperative mood starting with a capital letter and no trailing period:
*"esbuild: Return error when option x is not set"*, **NOT** *"returning some error."*

Most title/subjects should have a lower-cased prefix with a colon and one whitespace. The prefix can be:

* The name of the crate where (most of) the changes are made, as its directory in `crates/` names it (e.g. `images: Add the dither filter`)
* For a change outside `crates`, the area: `CI` (`.github/workflows`), `harness` (`tools/dev`), `sites.py`, `provenance`, `docs`.
* If this commit touches several crates with a common functional topic, use that as a prefix, e.g. `errors: Resolve correct line numbers`)
* If this commit touches many crates without a common functional topic, prefix with `all:` (e.g. `all: Apply the clippy lints of Rust 1.95`)
* If this is a documentation update, prefix with `docs:`.
* If nothing of the above applies, just leave the prefix out.
* Note that the above excludes nouns seen in other repositories, e.g. "chore:".

Also, if your commit references one or more GitHub issues, always end your commit message body with *See #1234* or *Fixes #1234*.
Replace *1234* with the GitHub issue ID. The last example will close the issue when the commit is merged into *main*.

An example:

```text
funcs: Add custom index function

Add a custom index template function that deviates from the stdlib simply by not
returning an "index out of range" error if an array, slice or string index is
out of range.  Instead, we just return nil values.  This should help make the
new default function more useful for fugo users.

Fixes #1949
```

###  Fetching the Sources From GitHub

Fugo is the Cargo workspace at the repository root. Building it needs Rust 1.96 or later (`rust-version` in `Cargo.toml`; CI uses 1.96.0) and a C compiler. Clone the repository:

```bash
mkdir $HOME/src
cd $HOME/src
git clone https://github.com/getfugo/fugo.git
cd fugo
```

Now, to make a change to fugo's source:

1. Create a new branch for your changes (the branch name is arbitrary):

    ```bash
    git checkout -b iss1234
    ```

1. After making your changes, commit them to your new branch:

    ```bash
    git commit -a -v
    ```

1. Fork fugo in GitHub.

1. Add your fork as a new remote (the remote name, "fork" in this example, is arbitrary):

    ```bash
    git remote add fork git@github.com:USERNAME/fugo.git
    ```

1. Push the changes to your new remote:

    ```bash
    git push --set-upstream fork iss1234
    ```

1. You're now ready to submit a PR based upon the new branch in your forked repository.

### Building and Testing Your Changes

`DEVELOPMENT.md` describes the workspace (layout, commands, CI and releases) and
`docs/rust-port/HANDOFF.md` the crates, the parity gates and the deviations from the Go implementation.

To build fugo (`target/release/fugo`):

```bash
cargo build --release --locked -p ssg-cli
```

To run the tests of the crate you are working on (package `ssg-<crate>`; `crates/cli` is package `ssg-cli`, which builds the binary `fugo`):

```bash
cargo test -p ssg-<crate>
```

The checks of the CI workflow, from the repository root (the last one checks the docs patches of `tools/rust-port/i01/patches.json` against `sites/docs/patches/`):

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast -- --show-output
tools/dev/licence-check.sh
python3 tools/dev/selftest.py
python3 tools/rust-port/i01/sites.py patches --check
```

The docs gates need the node modules the legacy docs site imports (Alpine.js, Turbo). Without them a gate prints `SKIPPED` and passes, so look for `SKIPPED` in the output before trusting a green run. To install them once (network, npm) into `tools/dev/node_modules` of the main checkout, where the tests find them:

```bash
tools/dev/node.sh
```

The tests compare fugo with the Go implementation through data it generated: the oracle fixtures (`testdata/oracle/`), the golden data of the test sites (`testdata/golden/`), `crates/build/tests/it/testsite-go.txtar`, `crates/highlight/tests/data/` with `crates/highlight/src/data/chroma-lexers.tsv`, `crates/funcs/tests/fixtures/remarshal/go.txt`, and `docs/data/docs.yaml` (written by the Go binary's `gen docshelper`). It is frozen at commit `44529028`, the last commit with the Go tree: do not edit it. The same holds for the Go outputs the old port recorded at `be02933a`, such as `testdata/corpus/minify/*.tsv` (`PROVENANCE.md`). To regenerate the data of `44529028`, run the old recipe in a worktree of that commit (`git worktree add <dir> 44529028`; `testdata/golden/README.md`, `crates/highlight/README.md`, `tools/dev/fixtures2json.py`) and copy the result back.
