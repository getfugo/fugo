---
title: Output formats
description: Render a page in several formats — HTML, RSS, JSON, plain text or your own — and write a template for each.
weight: 70
---

A page is rendered once per output format. By default the home page, sections, taxonomies and
terms are rendered as `html` and `rss`, regular pages as `html`.

{{< code-toggle file=config >}}
[outputs]
  home = ["html", "rss", "json"]
  section = ["html"]
  page = ["html"]
{{< /code-toggle >}}

A page can set its own with `outputs` in front matter.

## Built-in formats

`html`, `rss`, `json`, `calendar` (`.ics`), `csv`, `css`, `markdown`, `amp`, `robots`,
`sitemap`, `sitemapindex`, `webappmanifest`, plus `404` and `alias` for those pages.

## Templates per format

The format's name goes before the extension: `home.json.json` renders the home page as JSON,
`single.amp.html` renders pages for AMP. A template without a format name serves every format of
its media type.

```html {title="layouts/home.json.json"}
{{- [{"title": p.title, "url": p.permalink, "date": p.date} for p in site.regular_pages] | jsonify -}}
```

## Your own formats

{{< code-toggle file=config >}}
[outputFormats.searchindex]
  mediaType = "application/json"
  baseName = "search-index"
  isPlainText = true
  notAlternative = true

[outputs]
  home = ["html", "rss", "searchindex"]
{{< /code-toggle >}}

`mediaType`
: The media type; it gives the file extension.

`baseName`
: The file name without extension (default `index`).

`path`
: A directory to write the files to.

`isPlainText`
: Render without HTML escaping.

`isHTML`
: Treat it as HTML (for live reload and minification).

`notAlternative`
: Leave it out of `page.alternative_output_formats`.

`rel`
: The `rel` of its `<link>` (default `alternate`).

`permalinkable`
: Make `page.permalink` of a page rendered in this format point to this format.

`ugly`, `noUgly`
: Always use ugly URLs for this format (`/a.xml` instead of `/a/index.xml`), or never,
  whatever the site's `uglyURLs` says.

`root`
: Write the files at the publish root, not in a language's directory (`robots.txt`, the
  sitemap index).

`protocol`
: Replace the base URL's scheme in this format's links (`webcal://`).

`weight`
: Render order: formats with a non-zero weight first, by weight, then the others by name.

New media types are defined under `[mediaTypes]`: `[mediaTypes."text/x-hello"] suffixes =
["hello"]`.

## Linking formats

```html
{% for f in page.alternative_output_formats %}
  <link rel="{{ f.rel }}" type="{{ f.media_type.type }}" href="{{ f.permalink }}">
{% endfor %}
```

This documentation's search reads `/search-index.json`, an output format of the home page.
