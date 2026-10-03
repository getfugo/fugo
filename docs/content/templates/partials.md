---
title: Partials and components
description: Reuse template code with includes, partials that return values, cached partials and components with declared arguments.
weight: 40
---

Partials live in `layouts/_partials/`. There are three ways to use one.

## Include

`include` renders a partial with the caller's names — `page`, `site`, and any variable set
before it:

```html
{% include "_partials/header.html" %}
```

## partial()

`partial` renders a partial with the names you pass, and can return a value instead of text:

```html
{{ partial(name="card.html", p=p, size="small") }}
```

```html {title="layouts/_partials/cover.html"}
{#- The page's cover image: its resource named cover.*, or none. -#}
{{- return_value(value=p.resources | find_resource(pattern="cover.*")) -}}
```

```html
{% set cover = partial(name="cover.html", p=page) %}
{% if cover %}
  {% set thumb = cover | fill(width=600, height=400, format="webp") %}
  <img src="{{ thumb.rel_permalink }}" width="{{ thumb.width }}" height="{{ thumb.height }}" alt="">
{% endif %}
```

Every call still sees `page`, `site`, `build` and `lang`. `partial_cached(name=…, key=…)`
renders once per key and reuses the result: use it for the footer or a menu that is the same on
every page (`key=lang`).

## Components

A component is a partial with declared arguments, called like an HTML element:

```html {title="layouts/_partials/card.html"}
{% component card(p, size="medium", @lang) %}
<article class="card card-{{ size }}" lang="{{ lang }}">
  <a href="{{ p.rel_permalink }}">{{ p.title }}</a>
</article>
{% endcomponent card %}
```

```html
{% for p in page.pages %}
  {{ <card p={p} size="small" /> }}
{% endfor %}
```

Arguments are passed as `name={expression}`, `name="literal"` or just `name` (a variable of
that name). A component sees only its arguments: names prefixed with `@` in the declaration
(`@page`, `@site`, `@build`, `@lang`) are taken from the caller. A component that calls
functions which read the site or the page being rendered (`get_page`, `ref`, image processing)
declares `@__nh` too, or passes `page=` to them.

Calling a component that does not exist is an error when the templates load, before any page
is built.
