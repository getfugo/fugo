---
title: Embedded templates
description: The templates fugo ships — RSS, sitemap, robots.txt, aliases, render hooks, shortcodes and partials — and how to override them.
weight: 40
---

fugo ships a set of templates so that a site works without writing them: the RSS feed, the
sitemap, `robots.txt`, alias redirects, the default render hooks, the built-in shortcodes
Go-template sites expect and a few partials. They are loaded under the prefix `_embedded/`. A template of your project
or theme with the same name wins: to change the sitemap, add `layouts/sitemap.xml`.

{{% api-data table="embedded-templates" %}}

Call an embedded partial by its name: `{{ partial(name="pagination.html") }}`,
`{% include "_partials/opengraph.html" %}`. The shortcodes are used in content like your own:
`{{</* youtube id="0RKpf3rK57I" */>}}`. See [Shortcodes](/content-management/shortcodes/) for
their arguments.
