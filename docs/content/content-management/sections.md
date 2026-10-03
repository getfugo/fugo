---
title: Sections
description: Group pages into sections and nested sections, and navigate the section tree in templates.
weight: 50
---

A **section** is a group of pages under one directory: every directory directly below
`content/`, and a deeper directory that has an `_index.md`.

```text
content/
├── docs/                 section /docs/
│   ├── _index.md
│   ├── install.md
│   └── guides/           section /docs/guides/ (it has an _index.md)
│       ├── _index.md
│       └── deploy.md
└── blog/                 section /blog/ (top level: no _index.md needed)
    └── hello.md
```

A section's page uses `section.html` (or `list.html`) and lists its pages. `_index.md` gives
it a title, content and front matter; without one the title is the directory name, title-cased.

## In templates

| | |
|---|---|
| `page.section` | The top-level section key: `docs` for `/docs/guides/deploy/`. |
| `page.parent` | The section the page is in (none for the home page). |
| `page.current_section` | The page itself if it is a section, else its parent. |
| `page.first_section` | The top-level section the page is in. |
| `page.ancestors` | Every section above the page, nearest first. |
| `page.pages` | On a section: its pages and subsections. |
| `page.regular_pages` | On a section: its regular pages. |
| `page.regular_pages_recursive` | On a section: every regular page below it. |
| `page.sections` | On a section: its subsections. |
| `site.sections` | The top-level sections. |

Lists of pages hold *summary* values, without the relations above: use `p | deref` to get a
listed page's own `pages`. A breadcrumb trail:

```html
<nav aria-label="Breadcrumbs">
  {% for a in page.ancestors | reverse %}
    <a href="{{ a.rel_permalink }}">{{ a.link_title }}</a> ›
  {% endfor %}
  {{ page.title }}
</nav>
```

## Order

Lists are in the default order: by `weight` (pages without a weight last), then newest
`date` first, then link title. Sort otherwise with filters: `page.pages | by_title`,
`by_date | reverse`, `sort_by(attribute="params.order")`.

## Main sections

`site.main_sections` lists the sections of the main content — the `mainSections`
configuration, else the section with the most pages — for themes that show "recent posts"
without knowing the section's name.
