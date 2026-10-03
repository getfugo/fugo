---
title: Data sources
description: Use data files, remote JSON and CSV, and resources as data in templates, and turn data into pages with content adapters.
weight: 160
---

## Data files

Files in `data/` — JSON, TOML, YAML, CSV and XML — are `site.data`, by path:
`data/authors/jane.toml` is `site.data.authors.jane`.

```toml {title="data/authors/jane.toml"}
name = "Jane Doe"
links = ["https://example.org/jane"]
```

```html
{% set a = site.data.authors[page.params.author] %}
<p class="byline">By {{ a.name }}</p>
```

A CSV file is a list of rows, each a list of strings. Themes and modules can ship data files;
the project's win.

## Remote data

`get_remote` fetches a URL at build time; `unmarshal` parses it (JSON, TOML, YAML, CSV, XML,
detected from the media type or set with `format`):

```html
{% set r = get_remote(url="https://api.github.com/repos/getfugo/fugo") %}
{% if r %}
  {% set repo = r | unmarshal %}
  <p>{{ repo.stargazers_count }} stars</p>
{% endif %}
```

Responses are cached in the file cache (`caches.getresource`, see
[Caching](/configuration/caching/)), so a rebuild does not fetch again. `options` sets the
`method`, `headers` and `body`; a failed request fails the build unless `optional=true`, which
makes it a warning and returns none.

## Resources as data

A page resource or an asset parses the same way:
`page.resources | get_resource(name="table.csv") | unmarshal`,
`get_asset(path="data/stations.json") | unmarshal`.

## Data as pages

To create pages from data, use a [content adapter](/content-management/content-adapters/).
