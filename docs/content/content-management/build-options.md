---
title: Build options
description: Keep pages out of lists, render pages without publishing them, and control what is published with the build front matter.
weight: 180
---

The `build` front matter decides whether a page is rendered, listed and published.

{{< code-toggle file=content/sidebar fm=true >}}
title = "Sidebar"
[build]
  list = "never"
  render = "never"
  publishResources = false
{{< /code-toggle >}}

`render`
: `always` (default): the page is rendered and published. `never`: it is neither, but stays
  available to templates (`get_page`). `link`: it is not published, but has a URL.

`list`
: `always` (default): the page is in `page.pages`, `site.regular_pages` and other lists.
  `never`: in no list. `local`: only in its section's lists, not in site-wide ones.

`publishResources`
: Whether its page resources are copied to the output (default `true`). Resources the
  templates use (`rel_permalink`) are published anyway.

## Headless bundles

A *headless* bundle holds content other pages use, without being a page itself:

{{< code-toggle file=content/footer/index fm=true >}}
title = "Footer"
[build]
  list = "never"
  render = "never"
{{< /code-toggle >}}

```html
{% set footer = get_page(path="/footer") %}
{{ footer.content }}
{% for img in footer.resources %}…{% endfor %}
```

## For a whole section

Set `build` for every page below a section with `cascade`:

{{< code-toggle file=content/drafts/_index fm=true >}}
[cascade.build]
  list = "local"
  render = "always"
{{< /code-toggle >}}

## Drafts, future and expired pages

Pages with `draft: true`, a future `publishDate` or a past `expiryDate` are skipped, unless the
build asks for them: `fugo build --build-drafts --build-future --build-expired` (`-D -F -E`).
