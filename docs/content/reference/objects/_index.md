---
title: Objects
description: The values templates read — page, site, resources, menus, pagers and more — with every field and the Go-template name it replaces.
weight: 20
---

Templates read values: `page` and `site` in every layout, a [resource](/reference/objects/resource/)
returned by `get_asset`, a [menu entry](/reference/objects/menu-entry/) in a loop over
`site.menus.main`, a [pager](/reference/objects/pager/) from `paginator()`. Their fields use
snake case: `.Page.RelPermalink` of Go templates is `page.rel_permalink`, `.Site.Params.Title` is
`site.params.title`.

A field that has no value is `none`. Printing an undefined value is an error in Tera, so give
optional values a fallback: `{{ page.params.subtitle or "" }}`, or check them first:
`{% if page.description %}`.
