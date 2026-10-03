---
title: Templates
description: Write layouts, partials, shortcodes and render hooks in Tera, and learn how fugo picks the template for each page.
weight: 40
---

fugo renders pages with [Tera](https://keats.github.io/tera/) templates, under the layout names
Go-template sites use (`home.html`, `single.html`, `_partials/`). Start with the [introduction](/templates/introduction/) for Tera's syntax, then
the [lookup order](/templates/lookup-order/) for which file renders which page.

Run `fugo templates check` after you change templates: it reports syntax errors, unknown names
and functions, and pages no template can render, without building the site.
