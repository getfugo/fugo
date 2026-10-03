---
title: Front matter
description: The metadata at the top of a content file — title, dates, draft status, weight, URLs, menus, parameters — in YAML, TOML or JSON.
weight: 20
---

Front matter is metadata at the top of a content file. Write it in YAML between `---`, TOML
between `+++`, or JSON as an object:

{{< code-toggle file=content/posts/hello fm=true >}}
title = "Hello, fugo"
date = 2026-10-02T09:00:00Z
draft = false
tags = ["intro", "rust"]
[params]
  author = "Ada"
{{< /code-toggle >}}

Templates read the fields: `page.title`, `page.date`, `page.params.author`. A key fugo does
not know is a parameter: `author: Ada` at the top level is `page.params.author` too. Parameter
keys are lower case in templates.

## Fields

`aliases`
: URLs that redirect to the page, e.g. `["/old-url/"]`. See [URLs](/content-management/urls/#aliases).

`build`
: [Build options](/content-management/build-options/): whether the page is rendered, listed,
  and its resources published.

`cascade`
: Front matter that the page's descendants inherit. See [Cascade](#cascade).

`date`
: The date of the page, e.g. `2026-10-02` or `2026-10-02T09:00:00+07:00`.

`description`
: A description, for the `<meta name="description">` of the page and for listings.

`draft`
: `true` leaves the page out unless the build uses `--build-drafts` (`-D`).

`expiryDate`
: After this date the page is left out unless the build uses `--build-expired` (`-E`).

`keywords`
: A list of keywords.

`lastmod`
: The date of the last change.

`layout`
: A layout name: `layout: wide` uses `wide.html` instead of `single.html`.

`linkTitle`
: A shorter title for links and menus (`page.link_title`).

`markup`
: The content format, `md` (Markdown) or `html`, when the file extension does not say.

`menus`
: Adds the page to [menus](/content-management/menus/#in-front-matter): `menus: main`, or a map
  with `weight`, `parent`, `name`, `identifier`.

`outputs`
: The [output formats](/templates/output-formats/) of the page, e.g. `[html, json]`.

`params`
: Parameters, read as `page.params.<key>`.

`publishDate`
: Before this date the page is left out unless the build uses `--build-future` (`-F`).

`resources`
: Metadata for [page resources](/content-management/page-resources/#metadata): names, titles
  and parameters by file pattern.

`sitemap`
: The page's [sitemap](/templates/sitemap/) settings: `changeFreq`, `priority`, `disable`.

`slug`
: The last segment of the URL, replacing the file name.

`summary`
: The page's [summary](/content-management/summaries/), instead of the automatic one.

`title`
: The title.

`translationKey`
: Links [translations](/content-management/multilingual/#translate-content) that have different
  file names.

`type`
: The content type, used to look up templates instead of the section.

`url`
: The whole URL path of the page, e.g. `/about-us/`.

`weight`
: The position in lists: lower weights first. Pages without a weight come after those with one.

## Dates

`date`, `lastmod`, `publishDate` and `expiryDate` take a date (`2026-10-02`), a date and time
(`2026-10-02T09:00:00`), with a zone (`…+07:00` or `…Z`), or Unix seconds. A date without a
zone is in the site's `timeZone`, else UTC. Templates format dates with the `date` filter:

```text
{{ page.date | date(format="%B %-d, %Y") }}
```

Where each date comes from is configurable: `lastmod` can fall back to the date, a date can be
taken from the file name (`2026-10-02-my-post.md`). See [Front matter configuration](/configuration/front-matter/).

## Cascade

`cascade` sets front matter on a page's descendants. In a section's `_index.md`:

{{< code-toggle file=content/docs/_index fm=true >}}
title = "Documentation"
[cascade]
  [cascade.params]
    show_toc = true
  [cascade.target]
    kind = "page"
{{< /code-toggle >}}

Every regular page below `/docs/` gets `page.params.show_toc`. A page's own front matter wins
over a cascade. `target` restricts the cascade by `kind`, `path` (a glob like `/docs/api/**`),
`lang` and `environment`; a list of tables gives several cascades.

## Summary divider and content

Everything after the front matter is the content. <code>&lt;!-<wbr>-more-<wbr>-&gt;</code> marks the end of the
[summary](/content-management/summaries/).
