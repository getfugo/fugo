---
title: Troubleshooting
description: Read fugo's error messages, fix the common template errors, find missing layouts, and measure a slow build.
weight: 110
---

## Check templates first

```sh
fugo templates check
```

It loads every template without building, and reports Tera syntax errors, unknown filters and
functions, names a template cannot see, Go template syntax and legacy file names. It then lists
which template renders each kind of page, and pages that no template renders. Add
`--deny-warnings` in CI.

## Common errors

### A field is not defined

```text
ERROR build failed: home.html (/): error: Field `nope` is not defined. Available fields: draft, iscjklanguage, title
 --> home.html:1:19
  |
1 | <p>{{ page.params.nope }}</p>
  |                   ^^^^
```

Printing a value that does not exist is an error. Check the spelling (parameters are lower
case), or give a fallback: `{{ page.params.nope or "" }}`, `{% if page.params.nope %}`.
See [Missing values](/templates/introduction/#missing-values).

### Go template syntax

```text
layouts/single.html:1: Go template syntax `{{ .`: layouts must be Tera templates
```

The file is a Go template. Convert it with the
[conversion guide](/migrating/templates/).

### Legacy layout name

```text
layouts/_default/single.html: legacy layout name; rename it to single.html
```

Rename the file as the message says; see [Template lookup order](/templates/lookup-order/#old-names).

### Unknown filter or function

```text
ERROR build failed: loading the templates: error: Unknown filter `nosuch`
```

Check the name in the [function reference](/reference/functions/): fugo's names are snake
case, and some Go-template functions are filters (`x | markdownify`) or have new names.

### No layout for a page

```text
WARN  no layout for taxonomy page /tags in format html
```

No template matches the page. Add one (`taxonomy.html`, or `list.html` for every list page),
or turn the kind off: `disableKinds = ["taxonomy", "term"]`.

### npm packages cannot be installed

```text
installing the npm packages of …/package.json: …
```

fugo could not reach the registry, or a version in `package.json` does not exist. Check your
network, proxy and `.npmrc`. A `node_modules` that npm, pnpm or yarn installed is used as it is.

### Not allowed by security

```text
get_env: `HOME` is not allowed by security.funcs.getenv (allow it there, or define it in the project's .env file)
```

Allow it in [`[security]`](/configuration/security/), if you trust the template, or, for a value
of your own such as an API key, define it in the project's
[`.env` file](/configuration/introduction/#the-env-file).

## Debugging templates

`{{ value | jsonify }}` prints a value as JSON; inside `<pre>`, it shows what a template can read.
`log_warn(message=…)` writes to the build log. `fugo config` prints the resolved configuration.

## Slow builds

`FUGO_TIMINGS=1 fugo build` prints the time of each build phase. Usually the first build is slow
and the next ones fast: processed images and remote files are cached
([Caching](/configuration/caching/)). Keep `resources/_gen` between CI builds.

## Reporting a bug

Open an issue on [GitHub](https://github.com/getfugo/fugo/issues) with the fugo version
(`fugo version`), the error, and a small project that reproduces it.
