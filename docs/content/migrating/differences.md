---
title: Differences
description: What fugo does not support, what it does differently from the Go implementation, and which output differences to expect when you compare builds.
weight: 20
---

## Not supported

Go templates
: Layouts, shortcodes, render hooks, content adapters (`_content.gotmpl`) and
  `execute_as_template` assets are Tera. A Go template is an error that names its first Go tag.

Content formats
: Markdown and HTML only. AsciiDoc, reStructuredText, Pandoc and Org mode files are errors.

Module downloads
: Modules are not downloaded (no `go.mod`). Use `themes/`, `_vendor/` (a vendored copy of the
  modules) or a path; see [Modules and mounts](/configuration/module/).

Commands
: `fugo` has `build`, `server`, `templates check`, `config` and `version`. The Go
  implementation's `new`, `mod`, `deploy`, `gen`, `list`, `env`, `convert` and `import` commands
  do not exist.

Node.js tools
: No PostCSS, Babel or Tailwind CSS pipeline, and no build stats file (`buildStats`): fugo runs
  no programs. `minify` adds vendor prefixes, `js_build` compiles TypeScript and JSX for the
  browser targets, `purge_css` purges per page, and Tailwind's own CLI runs next to fugo (see
  [Tailwind CSS](/asset-pipelines/tailwind-css/)).

Git information
: `page.git_info` is always none and `:git` dates give nothing: fugo does not read Git history.

`image_colors`
: Not implemented yet (`.Colors` in Go templates).

Goldmark `extras` and `cjk`
: Accepted, not applied: no `==mark==`, `++insert++`, `~sub~`, `^sup^`, and no CJK line-break
  handling.

Server settings
: `[server]` headers and redirects are not read; every change rebuilds the whole site (there is
  no fast render); no TLS, `--openBrowser` or `--pprof`.

Some flags
: The Go implementation's logging and housekeeping flags (`--gc`, `--logLevel`,
  `--noBuildLock`, `--printI18nWarnings`, `--printPathWarnings`, `--printUnusedTemplates`,
  `--templateMetrics`, `--templateMetricsHints`) and the server's `--disableFastRender` and
  `--disableBrowserError` are usage errors: remove them from your command lines.

## Different by design

- Templates are Tera with the current layout names; older names are refused with the new one.
- Printing an undefined value is an error, not an empty string.
- Content is rendered before layouts: inside a shortcode, another page's content comes from
  `page_content(page=p)`.
- Pagination is explicit: a second `paginate` with another list is an error.
- YAML is YAML 1.2: `yes` and `no` are strings.
- Imaging settings, media types and output formats are per project, not per language.
- Settings that the Go implementation ignores silently (a `[caches]` entry that is not a table, an output format
  without a media type) are errors.
- `build.version` is fugo's version.
- Settings and the environment are not read from environment variables: `--environment` chooses
  the environment. Secrets that templates read go in the
  [`.env` files](/configuration/introduction/#the-env-file).

## Expected output differences

When you compare a fugo build with the Go implementation's, these are normal:

- **Minified bytes** (lightningcss, oxc and minify-html instead of tdewolff), and so the
  fingerprints of minified files.
- **Sass output**: grass follows Dart Sass, not LibSass; no source maps.
- **JavaScript bundles**: rolldown instead of esbuild.
- **Processed images**: equal to the eye, not to the byte; file names have fugo's own hashes.
- **Highlighted code**: the same colours and classes, with a few differences in span structure.
- **Typography**: characters where the Go implementation writes entities (`’` for `&rsquo;`).
- **Sort order** of titles in some languages: fugo uses newer Unicode collation data.
- The `generator` meta tag names fugo.
