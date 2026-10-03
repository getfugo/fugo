---
title: Content adapters
description: Create pages and page resources from data — files, remote APIs, generated reference — with a template in the content directory.
weight: 170
---

A content adapter is a template named `_content.html` in a directory of `content/`. It runs
before the site is assembled and adds pages and resources to its directory, as if they were
files there.

```html {title="content/books/_content.html"}
{%- for b in site.data.books %}
  {{- add_page(page={
    "path": b.slug,
    "title": b.title,
    "dates": {"date": b.published},
    "params": {"author": b.author, "isbn": b.isbn},
    "content": {"mediaType": "text/markdown", "value": b.summary}
  }) -}}
{%- endfor %}
```

Each book becomes `/books/<slug>/`, rendered with the section's layouts like any page, listed
in the section, in taxonomies, feeds and the sitemap.

## add_page

`path` (required)
: Relative to the adapter's directory: `intro`, `guides/install`. A path ending in a section
  (`kind: "section"`) adds a section.

`kind`
: `page` (default), `section` or `home`.

`content`
: `{"mediaType": …, "value": …}`: `text/markdown` (default), `text/html`.

`title`, `dates`, `params`, `build`, `cascade`, `outputs`, `weight`, …
: Any front matter field, but `lang`.

## add_resource

```html
{{- add_resource(resource={
  "path": "books/" ~ b.slug ~ "/cover.jpg",
  "content": {"mediaType": "image/jpeg", "value": get_remote(url=b.cover)}
}) -}}
```

`content.value` is a string, or a resource (which keeps its own URL). Resources at a page's
path belong to that page: `page.resources` and image processing see them.

## What an adapter can read

`site.data`, `site.params`, `get_remote`, `get_asset`, `unmarshal` and the other functions —
but not the site's pages: they do not exist yet. An adapter runs once per language; call
`enable_all_languages()` to have one adapter add pages to every language. `store_set` and
`store_get` (without `page=`) share values between those runs.

This documentation's [functions](/reference/functions/), [objects](/reference/objects/) and
[commands](/commands/) reference pages are content adapters over data files fugo generates.
