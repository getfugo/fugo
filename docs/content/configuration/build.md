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

fugo's minifiers are lightningcss (CSS), oxc (JavaScript) and minify-html (HTML). A table per
type sets their options:

{{< code-toggle file=config >}}
[minify]
  minifyOutput = true
  [minify.html]
    keepComments = true
  [minify.js]
    keepVarNames = true
{{< /code-toggle >}}

`html.keepComments`
: Keep every HTML comment (`false`).

`html.keepSpecialComments`
: Keep server-side-include comments, `<!--#…-->` (`true`).

`html.keepEndTags`, `html.keepDocumentTags`
: `false` leaves out optional closing tags, and `<html>` and `<head>` opening tags without
  attributes (`true`).

`html.keepDefaultAttrVals`
: `false` leaves out `type="text"` on `<input>` (`true`).

`html.templateDelims`
: `["{{", "}}"]` keeps `{{ }}`, `{% %}` and `{# #}` as written, `["<%", "%>"]` keeps `<% %>`.

`css.keepCSS2`
: Write colours in CSS files as `rgb()` and `rgba()` with commas, not as hex with alpha
  (`true`).

`js.keepVarNames`
: Do not rename local variables (`false`).

`svg.keepComments`
: Keep comments in SVG files (`false`).

`xml.keepWhitespace`
: Keep the whitespace of XML text as written (`false`).

The Go implementation's other minifier options (`keepQuotes`, `precision`, …) are accepted and
have no effect. Its name for these tables, `[minify.tdewolff]`, is still read, with a
deprecation notice.

In a page, each inline `<script>` is minified by its `type`: JavaScript (classic scripts and
modules) by oxc, JSON (`application/ld+json`, `application/json`, import maps) by the JSON
minifier. Other types, such as client-side templates (`text/x-template`), and scripts that do
not parse are kept as written.
