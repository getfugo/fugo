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

### Multilingual sites

A multilingual site, or one whose default language is in a subdirectory, gets one sitemap per
language and a `/sitemap.xml` index of them, which `robots.txt` names. Each sitemap is in its
language's directory, the default language's too, even when its pages are at the root:

```text
public/sitemap.xml        the index of the two below
public/en/sitemap.xml     https://example.org/about/, …
public/fr/sitemap.xml     https://example.org/fr/about/, …
```

A sitemap may list only the URLs below its own directory
([the sitemap protocol](https://www.sitemaps.org/protocol.html#location)): search engines can
skip the pages at the root that `/en/sitemap.xml` lists. Google accepts them when the sitemap
is submitted in Search Console. With `root` (fugo's own setting), every language's sitemap is
at the root instead, where it may list any page of the site:

{{< code-toggle file=config >}}
[sitemap]
  root = true
{{< /code-toggle >}}

```text
public/sitemap.xml        the index of the two below
public/sitemap-en.xml     https://example.org/about/, …
public/sitemap-fr.xml     https://example.org/fr/about/, …
```

Each is named after `filename`, with its language's key before the extension. The index,
`page.pages` and `p.sitemap` are as above. A site with one language and no index has its
sitemap at `/sitemap.xml` either way.

## robots.txt

With `enableRobotsTXT = true`, fugo writes `/robots.txt` allowing every crawler. Add
`layouts/robots.txt` to write your own; it is a template like any other:

```text {title="layouts/robots.txt"}
User-agent: *
{% for p in [x for x in site.pages if x.params.private] %}Disallow: {{ p.rel_permalink }}
{% endfor %}
Sitemap: {{ "sitemap.xml" | abs_url }}
```
