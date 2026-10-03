---
title: Build and minify
description: The build table, and HTML, CSS and JavaScript minification.
weight: 100
---

## Build

`[build]` is accepted for compatibility. `cacheBusters`, `useResourceCacheWhen` and
`noJSConfigInAssets` are checked and have no effect: fugo always runs a pipeline's steps, and
writes no `jsconfig.json`.

`buildStats` (and the older `writeStats`) is no longer supported, and a warning says so: fugo
writes no file of the classes and tags pages use. [`purge_css`](/asset-pipelines/purge-css/)
reads each page itself, and [Tailwind CSS](/asset-pipelines/tailwind-css/) finds the classes in
your layouts.

## Minify

`fugo build --minify`, or `minifyOutput`, minifies every HTML, CSS, JavaScript, JSON, SVG and
XML file fugo writes. `disableHTML`, `disableCSS`, `disableJS`, `disableJSON`, `disableSVG` and
`disableXML` turn off one type:

{{< code-toggle file=config >}}
[minify]
  minifyOutput = true
  disableXML = true
{{< /code-toggle >}}

fugo's minifiers are lightningcss (CSS), oxc (JavaScript) and minify-html (HTML). The
`[minify.tdewolff]` options of Go-template sites are accepted, and only some apply.
