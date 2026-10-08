# ssg-npm

A project's npm packages, without Node.js or npm, through Deno's npm installer used as a library:
`ensure_installed(project, cache)` installs the `dependencies`, `devDependencies` and
`optionalDependencies` of `<project>/package.json` into `<project>/node_modules`
(`deno_npm_installer`). It resolves against the registry of `.npmrc`, verifies each tarball's
integrity, uses the npm-style hoisted layout and installs only the optional packages of the
current platform. The cache directory is `<cache>/packages`. Nothing runs the packages'
programs: `js_build` and Sass read the installed files.

`ssg-cli` uses the crate behind its default feature `npm`: the install runs in
`ssg_build::Prepare` before the build reads the project, and in the server when it starts and
when `package.json` or `package-lock.json` changes.

User documentation: `docs/content/asset-pipelines/npm-packages.md`.

## API

```rust
pub const LOCK_FILE: &str = "npm.lock";
pub enum Installed {
    NoPackages, External(&'static str), UpToDate, Installed { elapsed: Duration },
    Replaced { owner: &'static str, reason: String, elapsed: Duration },
}
pub enum InstallError { Io { path, source }, PackageJson { path, message }, Install { path, message } }
pub fn ensure_installed(project: &Path, cache: &Path) -> Result<Installed, InstallError>;
```

## Behaviour

**Who owns `node_modules`.** The installer manages a `node_modules` that it wrote (Deno's
`.deno` directory is in it) or one that does not exist yet. A link (the test harness links
`tools/dev/node_modules`) is `External("a link")`: nothing is checked or changed.

Another package manager's `node_modules` (npm's, pnpm's or yarn's state file:
`.package-lock.json`, `.modules.yaml`, `.yarn-state.yml`, `.yarn-integrity`; or any other
non-empty directory) is `External(owner)` while it is up to date (`stale.rs`):

- each dependency of `package.json` is installed (`node_modules/<name>/package.json`) at a
  version its range allows (`deno_semver`'s npm ranges). Not checked: tags, `file:`, git and
  alias specifiers, and a missing optional dependency;
- for npm's only, each entry of `package-lock.json`'s `packages` (lockfile versions 2 and 3)
  is installed at its version. Not checked: links and missing optional entries.

When one is not, `ensure_installed` removes the directory (as `npm ci` does: no file of the old
versions stays) and installs: `Replaced { owner, reason }`, the reason naming the first package
out of date. The new `node_modules` is the installer's until the other manager writes its state
file again.

**Stamp.** `node_modules/.deno/.install-stamp` holds the SHA-256 of the installer version, the
platform, `package.json` and the lock file. When it matches, `ensure_installed` returns
`UpToDate` without reading the network.

**Lock file.** Deno's lockfile format, written to `npm.lock` next to `package.json` (`lock_arg`).
When `npm.lock` is missing, `package-lock.json` seeds it (`import_npm_lockfile`). A lock file
records the registry origin of packages from registries other than npmjs.org.

**Not done.** Install scripts never run (`NullLifecycleScriptsExecutor`). Workspaces of several
`package.json` files are not installed. Theme `package.json` files are not merged into the
project's.

## Gotchas

- Deno's workspace discovery caches `package.json` per thread
  (`node_resolver::PackageJsonThreadLocalCache`). `install` clears the cache first, so that a
  server reinstalling on the same thread sees the edited file.
- The Deno crates are pinned exactly, as one release (deno 2.9.7): `deno_resolver`,
  `node_resolver`, `deno_npm_installer`, `deno_npm_cache`, `deno_npmrc` and `deno_config` move
  together. Upgrade them all at once, to the versions a deno CLI release pins.

## Tests

`tests/it` (no network: `ssg_testkit::registry` serves package documents and tarballs on
`127.0.0.1`, and an `.npmrc` points at it):

- `install`: a hoisted install and its lock file; nothing fetched when up to date; installing
  again after `package.json` changes (and removing what left it); the lock file keeping versions;
  no packages; another manager's `node_modules` left alone while up to date, replaced when a
  package is missing or at a version `package.json` or `package-lock.json` does not give (and
  then installed at the version of `package-lock.json`); a linked `node_modules`; a missing
  package; a broken `package.json`.

`ssg-cli`'s `tests/it/npm.rs` runs the binary: a build that installs a package and bundles it
with `js_build` with no Node.js on `PATH`, a build that replaces npm's out-of-date
`node_modules`, an unreachable registry, and a server that installs again after `package.json`
or `package-lock.json` changes.
