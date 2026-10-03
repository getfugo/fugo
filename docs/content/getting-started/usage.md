---
title: Basic usage
description: The commands you use every day — build, serve, check templates — and the options that change what gets built.
weight: 30
---

## Build

```sh
fugo
```

Builds the site in the current directory into `public/`. The build prints one line with what it
did, and the time it took:

```text
pages 42 | files 51 (aliases 2) | resources 12 | processed images 6 | static files 9
Total in 128 ms
```

Common options:

`-s <dir>`
: The project directory, when it is not the current one.

`-d <dir>`
: Where to write the site (default `public`).

`--minify`
: Minify HTML, CSS, JavaScript, JSON, SVG and XML.

`-b <url>`
: Override `baseURL`, for example for a preview deployment.

`-e <environment>`
: The [environment](#environments) (default `production`).

`--clean-destination-dir`
: Remove files from the publish directory that the site no longer has.

## Drafts, future and expired content

A page with `draft: true`, a `publishDate` (or `date`) in the future, or an `expiryDate` in the
past is not built. Include them with:

```sh
fugo -D -F -E    # --build-drafts --build-future --build-expired
```

`--clock 2030-01-01T00:00:00Z` sets the build's "now", to see what the site will look like on a
given date.

## Serve

```sh
fugo server
```

Builds the site into memory and serves it at <http://localhost:1313/> with live reload: when you
save a content file, a template, an asset or the configuration, fugo rebuilds and the browser
reloads. A build error keeps the last good site and prints the error. `-p` changes the port,
`--bind 0.0.0.0` serves your network, and `--render-to-disk` writes the site to the publish
directory as it serves it. See [`fugo server`](/commands/fugo-server/).

## Check templates

```sh
fugo templates check
```

Loads every template of the project and its themes without building, and reports every problem
at once: syntax errors, unknown functions and filters, wrong keyword arguments, names a template
cannot see, and pages without a template. Add `--deny-warnings` in CI. See
[`fugo templates check`](/commands/fugo-templates-check/).

## Environments

The environment is `production` for builds and `development` for the server. Change it with
`-e staging`. Templates read it as `build.environment`
(`build.is_production`, `build.is_development`), and the configuration directory's
`config/<environment>/` applies on top of `config/_default/`.

## Inspect the configuration

```sh
fugo config
```

Prints the resolved configuration — files, environment and overrides merged — as JSON
(`--format toml` for TOML).
