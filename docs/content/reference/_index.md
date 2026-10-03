---
title: Reference
description: Every template function, filter and test, every template object and its fields, and the names each kind of template sees.
weight: 80
---

The reference is generated from fugo's source code, so it lists exactly what your templates can
call and read:

- [Functions, filters and tests](/reference/functions/): every name a template can call, with
  its arguments and the Go-template function it replaces.
- [Objects](/reference/objects/): the fields of `page`, `site`, resources, menus, pagers and the
  other values templates read.
- [Render contexts](/reference/contexts/): the top-level names of each kind of template, and the
  fields of render hooks.
- [Embedded templates](/reference/embedded-templates/): the templates fugo ships, which your
  project can override.
