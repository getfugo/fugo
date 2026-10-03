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
installs again when you save `package.json`.

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

fugo manages a `node_modules` it created itself, or one that does not exist yet. It leaves
`node_modules` alone if:

- npm, pnpm or yarn wrote it (their state files are inside);
- it is a link;
- it is not empty and fugo did not create it.

If you install with your own package manager, fugo uses what you installed. Delete
`node_modules` to let fugo install it again.

## Without the installer

A fugo built without the `npm` feature (`cargo build --no-default-features --features
goat,math`) installs nothing: install `node_modules` with npm, pnpm or yarn.
