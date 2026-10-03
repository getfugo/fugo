---
title: Pagination
description: Split long lists into pages with paginator and paginate, and render navigation with the embedded pagination partial.
weight: 80
---

On a list page, `paginator()` splits the page's list into pagers of `pagerSize` pages, written
at `/page/2/`, `/page/3/` and so on:

```html {title="layouts/list.html"}
{% extends "baseof.html" %}
{% block main %}
  {% set pager = paginator() %}
  {% for p in pager.pages %}
    <h2><a href="{{ p.rel_permalink }}">{{ p.title }}</a></h2>
    {{ p.summary }}
  {% endfor %}
  {% include "_partials/pagination.html" %}
{% endblock main %}
```

To paginate another list, call `paginate` instead:

```html
{% set pager = paginate(pages=[p for p in site.regular_pages if p.section == "posts"], size=5) %}
```

A page has one pagination: the first call decides it, and calling `paginate` again with another
list is an error.

## Configuration

{{< code-toggle file=config >}}
[pagination]
  pagerSize = 10
  path = "page"
  disableAliases = false
{{< /code-toggle >}}

`/page/1/` redirects to the list page itself, unless `disableAliases` is true.

## Navigation

The embedded partial renders Bootstrap-style navigation: `{% include
"_partials/pagination.html" %}`, or `{{ partial(name="pagination.html", format="terse") }}` for
fewer links. To write your own, use the [pager](/reference/objects/pager/)'s fields:

```html
<nav class="pager">
  {% if pager.has_prev %}<a href="{{ pager.prev.url }}">Newer</a>{% endif %}
  <span>Page {{ pager.page_number }} of {{ pager.total_pages }}</span>
  {% if pager.has_next %}<a href="{{ pager.next.url }}">Older</a>{% endif %}
</nav>
```
