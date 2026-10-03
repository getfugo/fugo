---
title: Front matter
description: Configure where page dates come from, and set front matter for many pages at once with cascade.
weight: 30
---

## Dates

`[frontmatter]` lists, for each date, where to look, in order:

{{< code-toggle file=config >}}
[frontmatter]
  date = ["date", "publishDate", "pubdate", "published", "lastmod", "modified"]
  lastmod = [":git", "lastmod", "modified", "date", "publishDate", "pubdate", "published"]
  publishDate = ["publishDate", "pubdate", "published", "date"]
  expiryDate = ["expiryDate", "unpublishdate"]
{{< /code-toggle >}}

These are the defaults. A list you set replaces the default list; the first source that has a
date wins.

A front matter field name
: That field: `date`, `published`, or any other.

`:filename`
: The date at the start of the file or bundle name: `2026-10-02-my-post.md` gives the date
  and the slug `my-post`.

`:fileModTime`
: The file's modification time.

`:git`
: Accepted, but fugo does not read Git history, so it gives no date: `lastmod` falls through
  to the next source.

`:default` in a list stands for the default list of that date.

## Cascade

`cascade` in the configuration sets front matter on every page it matches:

{{< code-toggle file=config >}}
[[cascade]]
  [cascade.params]
    banner = "images/docs-banner.jpg"
  [cascade.target]
    path = "/docs/**"
    kind = "page"
{{< /code-toggle >}}

`target` selects pages by `path` (a glob), `kind`, `lang` and `environment`. A page's own front
matter wins over a cascade; a section's `cascade` (in its `_index.md`) applies to the pages
below it. See [Front matter](/content-management/front-matter/#cascade).
