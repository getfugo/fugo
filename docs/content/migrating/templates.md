---
title: Converting templates
description: How to turn Go templates into Tera templates for fugo — a side-by-side table of syntax and functions, and the rules that catch most mistakes.
weight: 10
---

A Go template and its fugo version side by side:

```go-html-template {title="Go template: layouts/list.html"}
{{ define "main" }}
  <h1>{{ .Title }}</h1>
  {{ range where .Pages "Params.featured" true }}
    <h2><a href="{{ .RelPermalink }}">{{ .LinkTitle }}</a></h2>
    {{ with .Params.subtitle }}<p>{{ . }}</p>{{ end }}
  {{ end }}
  {{ partial "pagination.html" . }}
{{ end }}
```

```html {title="fugo: layouts/list.html"}
{% extends "baseof.html" %}
{% block main %}
  <h1>{{ page.title }}</h1>
  {% for p in [x for x in page.pages if x.params.featured] %}
    <h2><a href="{{ p.rel_permalink }}">{{ p.link_title }}</a></h2>
    {% if p.params.subtitle %}<p>{{ p.params.subtitle }}</p>{% endif %}
  {% endfor %}
  {% include "_partials/pagination.html" %}
{% endblock main %}
```

The main changes: the context is named (`page`, `site`) instead of `.`; fields are snake case
(`.RelPermalink` → `rel_permalink`); `define`/`block` become `extends` and `block`; `range`,
`with` and `where` become `for`, `if` and comprehensions.

## Syntax and functions

{{% api-data table="syntax" %}}

Every function has its own page in the [function reference](/reference/functions/), with the
Go-template name it replaces. Every object field lists its Go-template name in the
[object reference](/reference/objects/).

## Conversion rules

{{% api-data table="conversion-rules" %}}

## Tera facts worth knowing

{{% api-data table="tera-facts" %}}
