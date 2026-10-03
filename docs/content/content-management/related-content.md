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

## Configuration

The default uses `keywords` (weight 100), `date` (10) and `tags` (80):

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
: The minimum score, from 0 to 100.

`includeNewer`
: Include pages newer than the page.

`toLower`
: Compare values case-insensitively.

`indices`
: What to compare: a front matter key or a taxonomy (`name`), how much it counts
  (`weight`), and `type = "fragments"` to compare heading ids, `pattern` for dates,
  `cardinalityThreshold`, `applyFilter`.

`indices=["tags"]` limits a call to some indices.
