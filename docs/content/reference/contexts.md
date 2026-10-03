---
title: Render contexts
description: The top-level names each kind of template sees, and the fields of every render hook.
weight: 30
---

What a template can read depends on what is being rendered. A layout sees `page`, `site`,
`build` and `lang`; a shortcode also sees `shortcode` and `inner`; a render hook sees its node's
fields directly (`destination`, `text`, …). A name a template uses outside its context is
reported by [`fugo templates check`](/commands/fugo-templates-check/).

## Templates and their names

{{% api-data table="contexts" %}}

## What the names hold

{{% api-data table="context-names" %}}

`__nh` is fugo's render scope. You never print it; site-bound functions such as `get_page`,
`paginate` or `partial` read it. A [component](/templates/partials/#components) does not see
the caller's names, so a component that calls a site-bound function declares `@__nh`, or is
passed `page=`.

## Render hook fields

A render hook's fields are top-level names: in `_markup/render-link.html`, write
`{{ destination }}`, not `{{ .Destination }}`.

{{% api-data table="hook-fields" %}}
