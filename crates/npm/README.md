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
when `package.json` changes.

User documentation: `docs/content/asset-pipelines/npm-packages.md`.

## API

```rust
pub const LOCK_FILE: &str = "npm.lock";
pub enum Installed { NoPackages, External(&'static str), UpToDate, Installed { elapsed: Duration } }
pub enum InstallError { Io { path, source }, PackageJson { path, message }, Install { path, message } }
pub fn ensure_installed(project: &Path, cache: &Path) -> Result<Installed, InstallError>;
```

## Behaviour

**Who owns `node_modules`.** The installer manages a `node_modules` that it wrote (Deno's
`.deno` directory is in it) or one that does not exist yet. It reports `External` and changes
nothing in these cases:

- a link (the test harness links `tools/dev/node_modules`);
- a directory with npm's, pnpm's or yarn's state file (`.package-lock.json`, `.modules.yaml`,
  `.yarn-state.yml`, `.yarn-integrity`);
- any other non-empty directory.

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
  no packages; another manager's or a linked `node_modules`; a missing package; a broken
  `package.json`.

`ssg-cli`'s `tests/it/npm.rs` runs the binary: a build that installs a package and bundles it
with `js_build` with no Node.js on `PATH`, an unreachable registry, and a server that installs
again after `package.json` changes.
