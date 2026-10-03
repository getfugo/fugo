---
title: Template lookup order
description: How fugo chooses the template for each page and output format — by kind, layout, path, language and format — and the layout names it knows.
weight: 20
---

fugo picks each page's template by comparing every template's name with what the page needs:
the best match wins.

## Template names

```text
layouts/
├── baseof.html         the base template the others extend
├── home.html           the home page
├── section.html        section pages
├── taxonomy.html       taxonomy pages (/tags/)
├── term.html           term pages (/tags/rust/)
├── list.html           any list page without a more specific template
├── single.html         regular pages
├── all.html            any page without a more specific template
├── 404.html            the 404 page
├── _partials/          partials
├── _shortcodes/        shortcodes
├── _markup/            render hooks
└── docs/               templates for pages under /docs/
    ├── single.html
    └── _shortcodes/    shortcodes only pages under /docs/ can use
```

A name can carry a language and an output format before its extension:
`single.th.html`, `home.rss.xml`, `list.json.json`.

## What matches

The page's kind
: `home.html`, `section.html`, `taxonomy.html` and `term.html` for their kind; `list.html` for
  all four; `single.html` for regular pages; `all.html` for every kind.

The page's layout
: Front matter `layout: landing` selects `landing.html`; it beats the standard names.

The page's path
: A template in `layouts/docs/` applies to the pages under `content/docs/` (and their type
  `docs`, see below). The closest directory wins.

The language and output format
: `single.th.html` for Thai pages, `home.rss.xml` for the home page's RSS feed. A template
  without a format serves the formats of its media type.

Front matter `type: post` makes a page look up templates in `layouts/post/` as if it were in that
section.

## Examples

| Page | Templates, best first |
|---|---|
| `/` (HTML) | `home.html`, `list.html`, `all.html` |
| `/` (RSS) | `home.rss.xml`, `rss.xml`, then fugo's embedded RSS template |
| `/docs/` | `docs/section.html`, `docs/list.html`, `section.html`, `list.html`, `all.html` |
| `/docs/install/` | `docs/single.html`, `single.html`, `all.html` |
| `/docs/install/` with `layout: wide` | `docs/wide.html`, `wide.html`, `docs/single.html`, … |
| `/tags/rust/` | `term.html`, `list.html`, `all.html` |

## Old names

Older layout names are refused, with the name to use: `_default/single.html` →
`single.html`, `partials/` → `_partials/`, `shortcodes/` → `_shortcodes/`, `index.html` →
`home.html`, `taxonomy/list.html` and `term/term.html` → `term.html`. Templates written in Go's
template language are refused too; see [Migrating from Go templates](/migrating/).

`fugo templates check --coverage` shows which template renders each kind of page.
