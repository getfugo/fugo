---
title: Links and cross references
description: Link to other pages by their content path, so links survive URL changes, and find broken links at build time.
weight: 100
---

## Markdown links

Link to another page by its content path:

```md
See the [installation guide](/docs/install/) or the [next post](../second-post/).
```

By default a Markdown link is written as it is. To resolve link destinations, use the embedded
link [render hook](/templates/render-hooks/):

{{< code-toggle file=config >}}
[markup.goldmark.renderHooks.link]
  useEmbedded = "fallback"
{{< /code-toggle >}}

It resolves a relative destination to a page, a page resource or an asset and writes its URL,
so the link stays right when a page's URL changes (`slug`, `url`, permalinks). A query and a
fragment (`/docs/install/#linux`) are kept; destinations that do not resolve are written as
they are.

`useEmbedded`
: `fallback`: the embedded hook unless the project or a theme has its own; `always`: only
  the embedded hook; `never`; `auto` (the default): `fallback` for multilingual sites served
  from one host, else `never`. The image hook (`renderHooks.image`) works the same way.

To warn about broken links, write your own hook: this documentation's
`layouts/_markup/render-link.html` calls `log_warn` when a link resolves to nothing, and CI
fails the build on warnings.

## `ref` and `relref`

The `ref` and `relref` shortcodes give a page's absolute or relative URL, and fail the build
when the page does not exist:

```md
[Install]({{</* relref "/docs/install" */>}})
[Linux]({{</* relref "/docs/install#linux" */>}})
[French]({{</* relref path="/docs/install" lang="fr" */>}})
```

In templates, use the functions `ref(path=…)` and `rel_ref(path=…)`, or `get_page(path=…)`.
`refLinksErrorLevel = "warning"` turns errors into warnings; `refLinksNotFoundURL` is the URL
written instead.

## Heading ids

Headings get ids from their text (`## Install on Linux` is `#install-on-linux`); set one with
an attribute: `## Linux {#linux}`.
