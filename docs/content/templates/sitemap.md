---
title: Sitemaps and robots.txt
description: The sitemap fugo writes for search engines, how to configure or replace it, and robots.txt.
weight: 90
---

## Sitemap

fugo writes `/sitemap.xml` with every page that has a URL, from its embedded template. A
multilingual site gets one sitemap per language and a `/sitemap.xml` index of them.

{{< code-toggle file=config >}}
[sitemap]
  changeFreq = "weekly"
  priority = 0.5
  filename = "sitemap.xml"
  disable = false
{{< /code-toggle >}}

A page can change its entry, or leave it out:

{{< code-toggle file=content/search fm=true >}}
title = "Search"
[sitemap]
  disable = true
{{< /code-toggle >}}

To change the XML, add `layouts/sitemap.xml` (and `layouts/sitemapindex.xml`); `page.pages`
holds the pages and `p.sitemap` each page's settings.

## robots.txt

With `enableRobotsTXT = true`, fugo writes `/robots.txt` allowing every crawler. Add
`layouts/robots.txt` to write your own; it is a template like any other:

```text {title="layouts/robots.txt"}
User-agent: *
{% for p in [x for x in site.pages if x.params.private] %}Disallow: {{ p.rel_permalink }}
{% endfor %}
Sitemap: {{ "sitemap.xml" | abs_url }}
```
