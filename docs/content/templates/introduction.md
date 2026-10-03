---
title: Introduction to templates
description: Tera's syntax as fugo uses it — values, filters, functions, tests, control flow, comprehensions, escaping — and the names every template can read.
weight: 10
---

A template is HTML (or XML, JSON, text) with Tera tags:

`{{ expression }}`
: Prints a value.

`{% statement %}`
: Control flow, variables, blocks, includes.

`{# comment #}`
: Removed from the output.

A dash trims whitespace on its side: `{%- if x -%}`.

## Values

```html
<h1>{{ page.title }}</h1>
<time datetime="{{ page.date | date(format="%Y-%m-%d") }}">{{ page.date | date(style="long") }}</time>
<p>{{ page.params.subtitle or "" }}</p>
```

Every template sees a few top-level names: `page`, `site`, `build` (the version and
environment), `lang`; shortcodes add `shortcode` and `inner`, render hooks their fields. See
[Template contexts](/reference/contexts/) for each kind of template.

Fields use dots, keys of maps brackets or dots: `site.data.authors[name]`. Parameters are lower
case: `title: My Post` and `myParam: 1` in front matter are `page.title` and
`page.params.myparam`.

## Filters, functions and tests

```html
{{ page.title | upper }}
{{ page.summary | plainify | truncate(length=120) }}
{% set img = page.resources | get_resource(name="cover.jpg") %}
{% set about = get_page(path="/about") %}
{% if page.rel_permalink is starting_with(pat="/docs/") %}…{% endif %}
```

Filters transform the value on their left, functions stand alone, tests answer `is` questions.
Arguments are named. The [function reference](/reference/functions/) lists all of them.

## Operators

`+ - * / % //` (integer division), `~` (string concatenation), `== != < <= > >=`, `and or not`,
`in` (`"x" in list`, `"key" in map`, `"sub" in string`), and the conditional
`a if condition else b`.

## Control flow

```html
{% if page.params.toc and page.table_of_contents %}
  {{ page.table_of_contents }}
{% elif page.description %}
  <p>{{ page.description }}</p>
{% endif %}

{% for p in page.pages %}
  <li{% if loop.first %} class="first"{% endif %}>{{ loop.index }}. {{ p.title }}</li>
{% endfor %}

{% for key, value in page.params %}…{% endfor %}
```

`loop.index` (from 1), `loop.index0`, `loop.first` and `loop.last` are available in loops.

## Variables

`{% set x = … %}` sets a variable for the rest of the template — but inside a `for` loop, only
for that iteration. To keep a value beyond a loop, use `set_global`, or better, compute it with
a comprehension:

```html
{% set drafts = [p for p in site.regular_pages if p.draft] %}
{% set titles = [p.title for p in page.pages] | join(sep=", ") %}
{% set by_year = page.pages | group_by_date(format="%Y") %}
```

Maps and lists are written as literals: `{"width": 800, "format": "webp"}`, `[1, 2, 3]`.

## Missing values

Printing a value that does not exist is an error, so a misspelt name never prints nothing
silently. When a value may be missing, give a fallback or test it:

```html
{{ page.params.author or "Anonymous" }}
{{ page.parent?.title or "" }}
{% if page.params.image is defined %}…{% endif %}
```

`?.` reads a field of a value that may be none.

## Escaping

In HTML, XML and SVG templates, printed values are HTML-escaped. Content, summaries, the table
of contents and other HTML fields are already safe; mark your own HTML with `safe`:
`{{ html_string | safe }}`. In a `<script>`, print data with `jsonify | safe`.

## Lists of pages

Lists such as `page.pages` and `site.regular_pages` hold *summary* page values: titles, URLs,
dates, params, summaries — but not their own lists and relations. `p | deref` gives the full
page value when you need `p.pages` or `p.parent`. Parenthesised expressions take no `.` after
them: write `{% set full = p | deref %}{{ full.pages | length }}`.

## Raw text

`{% raw %}…{% endraw %}` prints its content as is, Tera tags included.
