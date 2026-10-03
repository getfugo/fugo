---
title: RSS feeds
description: The RSS feeds fugo writes for the home page, sections and taxonomies, and how to limit, link or replace them.
weight: 100
---

The home page, sections, taxonomies and terms have the `rss` output format by default, so
`/index.xml`, `/posts/index.xml` and `/tags/rust/index.xml` are RSS 2.0 feeds: the home
page's of every regular page, a section's of its regular pages, a term's of its pages.

{{< code-toggle file=config >}}
[services.rss]
  limit = 20
{{< /code-toggle >}}

`limit` caps the number of items (`-1`, the default, means all). Feeds carry each page's
summary; the site's `params.author` (a name or `{name, email}`) fills the author fields.

Link a page's feed in `<head>`:

```html
{% if page.output_formats.rss is defined %}
  {% set rss = page.output_formats.rss %}
  <link rel="alternate" type="application/rss+xml" href="{{ rss.rel_permalink }}" title="{{ site.title }}">
{% endif %}
```

To change the feed, add `layouts/rss.xml` for every feed, or `home.rss.xml`,
`section.rss.xml` for one kind. To have no feeds, leave `rss` out of `[outputs]`, or
`disableKinds = ["rss"]`.
