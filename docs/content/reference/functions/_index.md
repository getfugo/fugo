---
title: Functions, filters and tests
linkTitle: Functions
description: Every name a fugo template can call, grouped by topic, with its keyword arguments and the Go-template function it replaces.
weight: 10
---

fugo templates are [Tera](https://keats.github.io/tera/) templates. A name is one of three
kinds:

Filter
: Transforms a value: `{{ page.title | upper }}`, `{{ text | truncate(length=80) }}`.

Function
: Called on its own: `{{ now() | date(format="%Y") }}`, `{% set p = get_page(path="/about") %}`.

Test
: Checks a value in a condition: `{% if page.params.tags is defined %}`,
  `{% if n is divisible_by(divisor=3) %}`.

Arguments are always passed by keyword (`truncate(length=80)`); an argument shown with `=?` is
optional. Some names are only available in some kinds of template: `paginate` only in layouts,
`arg` only in shortcodes, `add_page` only in content adapters. Names marked *safe* return HTML
that fugo does not escape.

Each group below lists its names; every name has its own page.
