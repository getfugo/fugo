---
title: Related content
description: List pages related to the current one by keywords, tags, dates or headings, with configurable indices and weights.
weight: 190
---

`related` scores the pages of a list against a page by the values they share, and returns the
best matches:

```html
{% set related = related(pages=site.regular_pages, page=page, limit=5) %}
{% if related %}
  <h2>See also</h2>
  <ul>
    {% for p in related %}<li><a href="{{ p.rel_permalink }}">{{ p.title }}</a></li>{% endfor %}
  </ul>
{% endif %}
```

`page` defaults to the page being rendered. The page itself is never in the result.

## Configuration

Each index compares one value of the pages: a front matter key or a taxonomy. The default uses
`keywords` (weight 100), `date` (10) and, when the site has a `tags` taxonomy, `tags` (80):

{{< code-toggle file=config >}}
[related]
  includeNewer = false
  threshold = 80
  toLower = false
  [[related.indices]]
    name = "keywords"
    weight = 100
  [[related.indices]]
    name = "tags"
    weight = 80
  [[related.indices]]
    name = "date"
    weight = 10
{{< /code-toggle >}}

`threshold`
: The minimum score, from 0 to 100 (below).

`includeNewer`
: Include pages published after the page. Without it, the oldest page has no related pages.

`toLower`
: Compare values case-insensitively, in every index.

`indices`
: What to compare:
  - `name`: the front matter key or taxonomy, such as `tags`, `keywords`, `date`, `section` or
    a param.
  - `weight`: how much a shared value counts; 0 turns the index off.
  - `type`: `basic` (the default) compares values; `fragments` compares heading ids
    ([below](#headings)).
  - `pattern`: for a date index, the Go layout the dates are compared in: `2006` (the default)
    relates the pages of the same year, `2006-01` of the same month.
  - `cardinalityThreshold`: from 0 to 100; leave out the values that more than this percentage
    of the pages with a value in the index have (a tag on every page relates nothing). 0 keeps
    them all.
  - `toLower`: compare this index's values case-insensitively.
  - `applyFilter`: for a fragments index, give each result only its matched headings.

The configuration of each language is its own, under `[languages.<key>.related]`.

## Scores

Each value a page shares with the current one adds the weight of its index. A page that shares
`n` values is related when the average weight, on a scale of 0 to 100 between the lowest and
the highest weight (with 0), is at least `threshold / n`. The more values a page shares, the
lower the bar: with the default configuration, a page that shares only the year (10) is not
related, one that shares a tag (80) is, and so is one that shares a tag and the year
(90 / 2 = 45, at least 80 / 2 = 40).

The result is sorted by the summed weight, then the newest first, then by name.

## Searching

`related` takes more than the page:

`indices`
: Search only these indices: `indices=["tags"]`.

`keywords`
: Values to search, by index name, besides the page's: `keywords={"tags": ["rust", "cli"]}`.
  A single value is a list of one.

`fragments`
: Heading ids to search in the fragments indices: `fragments=["installation"]`.

`page`
: `page=none` searches the `keywords` and `fragments` alone, with the configuration of the
  render's language:

```html
{% set rust = related(pages=site.regular_pages, page=none, keywords={"tags": ["rust"]}) %}
```

`limit`
: At most this many pages.

## Headings

An index with `type = "fragments"` compares the ids of the pages' headings (`## Install` has
the id `install`) and the values of its front matter key, read as heading ids: a page with
`fragmentrefs = ["install"]` is related to the pages with an `install` heading.

{{< code-toggle file=config >}}
[related]
  includeNewer = true
  [[related.indices]]
    name = "fragmentrefs"
    type = "fragments"
    applyFilter = true
    weight = 80
{{< /code-toggle >}}

With `applyFilter`, when headings matched, the `fragments` of each page in the result holds
only those headings, so a list can link to the sections themselves:

```html
{% for p in related(pages=site.regular_pages, limit=5) %}
  {% for h in p.fragments.headings %}
    <a href="{{ p.rel_permalink }}#{{ h.id }}">{{ p.title }}: {{ h.title | safe }}</a>
  {% endfor %}
{% endfor %}
```

Each heading has its `id`, `level`, `title` (HTML) and the `headings` below it;
`fragments.identifiers` lists the ids. A page that another index found lists the matched
headings it has, often none. When no heading matched, or without `applyFilter`, the pages are
as everywhere else: in a layout, `fragments` holds all their headings; in a shortcode,
`page_fragments(page=p)` gives them.

Comparing headings needs the headings of every page in the list, which fugo reads from their
content once per list. It works in shortcodes too: a page's own headings then come from its
content without its shortcodes.
