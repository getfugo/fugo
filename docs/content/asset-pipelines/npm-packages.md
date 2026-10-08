---
title: npm packages
description: List npm packages in package.json; fugo installs them when it builds, without Node.js or npm.
weight: 60
---

Put the npm packages a site needs in `package.json`, next to the configuration file:

```json {title="package.json"}
{
  "private": true,
  "dependencies": { "alpinejs": "^3.14.9", "bootstrap": "^5.3.3" }
}
```

`fugo build` and `fugo server` install them into `node_modules` before they read the project.
You do not need Node.js or npm:

- [`js_build`](/asset-pipelines/js-build/) bundles what you import from them;
- [mounts](/configuration/module/) can use their files (`node_modules/bootstrap/scss`).

fugo runs none of their programs.

```text
Installed the npm packages of package.json in 2102 ms
```

The next build installs nothing until `package.json` or the lock file changes. `fugo server`
installs again when you save `package.json`, and looks at `package-lock.json` again when it
changes (see below).

## What gets installed

fugo installs `dependencies`, `devDependencies` and `optionalDependencies`, the way npm lays them
out: one `node_modules` with every package hoisted as far up as its version allows, and each
program in `node_modules/.bin`. It installs only the optional packages for your platform, such
as a native build for your operating system and processor.

Packages come from the npm registry, or from the registry and credentials in the project's
`.npmrc` or your `~/.npmrc`. Each download is checked against its integrity hash. Downloads are
kept in `:cacheDir/packages` (see [Caching](/configuration/caching/)), so a second project with
the same packages installs without the network.

Install scripts (`preinstall`, `install`, `postinstall`) never run.

## The lock file

fugo writes the versions it installed to `npm.lock`, next to `package.json`. The file is in
Deno's lockfile format, because fugo's installer is Deno's. Commit it: a fresh checkout installs
the same versions. When `npm.lock` does not exist, fugo starts from the versions in
`package-lock.json`, if you have one.

To upgrade, change the range in `package.json`, or delete `npm.lock`.

## Using npm, pnpm or yarn instead

fugo manages a `node_modules` it created itself, or one that does not exist yet. A
`node_modules` that npm, pnpm or yarn wrote (their state files are inside), or that is not empty
and that fugo did not create, fugo uses as long as it has the packages of `package.json`:

- each package is installed, at a version its range in `package.json` allows;
- if npm wrote it and you have a `package-lock.json`, each package is at the version the lock
  file gives.

When a package is missing or at another version, fugo deletes `node_modules`, as `npm ci` does,
and installs the packages itself, so a build never uses packages older than the ones you named:

```text
Installed the npm packages of package.json in 2102 ms: the node_modules npm wrote was out of date (@fortawesome/fontawesome-free 6.7.2 is installed, package.json wants 7.3.1)
```

The `node_modules` is then fugo's, until your package manager installs again. fugo runs no
install scripts: if a program you run yourself needs one, install again with your package
manager. fugo does not check dependencies that name a tag (`latest`), a path or a git
repository, nor an optional dependency that is not installed (one for another platform).

fugo leaves a `node_modules` that is a link alone, whatever it holds.

## Without the installer

A fugo built without the `npm` feature (`cargo build --no-default-features --features
goat,math`) installs nothing: install `node_modules` with npm, pnpm or yarn.
