---
title: Base templates and blocks
description: Define the outer page once in baseof.html and fill its blocks from each layout.
weight: 30
---

`baseof.html` holds the outer page — `<head>`, header, footer — and defines blocks that layouts
fill:

```html {title="layouts/baseof.html"}
<!doctype html>
<html lang="{{ site.language_code or lang }}">
<head>
  <meta charset="utf-8">
  <title>{% block title %}{{ page.title }} | {{ site.title }}{% endblock title %}</title>
  {% include "_partials/head.html" %}
</head>
<body>
  {% include "_partials/header.html" %}
  <main>{% block main %}{% endblock main %}</main>
  {% include "_partials/footer.html" %}
</body>
</html>
```

A layout extends it with its first tag and fills the blocks:

```html {title="layouts/single.html"}
{% extends "baseof.html" %}
{% block main %}
  <article>
    <h1>{{ page.title }}</h1>
    {{ page.content }}
  </article>
{% endblock main %}
```

Blocks the layout does not fill keep the base template's content; `{{ super() }}` prints it
inside an override. A layout can only fill blocks its base defines.

## Several base templates

`{% extends "baseof.html" %}` means "the best base template for this page", chosen like a
layout: `layouts/docs/baseof.html` for pages under `/docs/`, `baseof.term.html` for term pages,
`baseof.th.html` for Thai pages. A layout that extends another file by name extends exactly that
file.
