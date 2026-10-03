---
title: Taxonomies
description: Classify content with tags, categories or your own taxonomies, and list terms and their pages.
weight: 60
---

A taxonomy groups pages by *terms*: the taxonomy `tags` with the terms `rust` and `web`.
fugo creates a page for each taxonomy (`/tags/`) and each term (`/tags/rust/`).

## Configure taxonomies

The default taxonomies are `tags` and `categories`. Define your own, singular name first:

{{< code-toggle file=config >}}
[taxonomies]
  tag = "tags"
  category = "categories"
  series = "series"
{{< /code-toggle >}}

Defining `[taxonomies]` replaces the defaults; an empty table, or `disableKinds = ["taxonomy",
"term"]`, turns taxonomies off.

## Assign terms

{{< code-toggle file=content/posts/hello fm=true >}}
title = "Hello"
tags = ["rust", "web"]
series = ["Getting started"]
{{< /code-toggle >}}

A term's key is its name made URL-safe: `Getting started` is `/series/getting-started/`. A
page's position in a term's list can be set with a weight: `tags_weight = 10`.

## Templates

A taxonomy page uses `taxonomy.html`, a term page `term.html` (both fall back to `list.html`).
On a taxonomy page, `page.taxonomy` has the terms:

```html {title="layouts/taxonomy.html"}
<h1>{{ page.title }}</h1>
<ul>
  {% for t in page.taxonomy.terms | by_count %}
    <li><a href="{{ t.page.rel_permalink }}">{{ t.name }}</a> ({{ t.count }})</li>
  {% endfor %}
</ul>
```

A term page lists its pages as any list page does: `page.pages`, `page.term.name`.

A page's terms, e.g. in `single.html`:

```html
{% for t in page.terms.tags %}
  <a href="{{ t.rel_permalink }}">#{{ t.title }}</a>
{% endfor %}
```

Every term of a taxonomy, anywhere: `site.taxonomies.tags` maps each term's key to its name,
page, count and pages.

## Term pages with content

Add `content/tags/rust/_index.md` to give a term page a title, a description and content.

## Hierarchical taxonomies

Terms can form a tree, such as product types or places. Turn it on per taxonomy with the
table form of `[taxonomies]`:

{{< code-toggle file=config >}}
[taxonomies]
  tag = "tags"
  [taxonomies.category]
    plural = "categories"
    hierarchical = true
{{< /code-toggle >}}

A `/` in a term nests it: `categories = ["snacks/chips"]` is the term `/categories/snacks/chips/`
below `/categories/snacks/`. In a hierarchical taxonomy:

- Every term above a term exists: fugo makes the missing term pages, titled with their segment
  as written (`snacks`, `Snacks` once titled).
- A term lists its own pages and those of every term below it, each page once (`page.pages`,
  and the term's `count` and `pages`). The smallest weight of a page counts.
- The taxonomy page lists the top-level terms. `page.sections` of the taxonomy page and of a
  term page are the terms directly below it, and `page.parent` and `page.ancestors` follow the
  tree, so breadcrumbs need no change.
- A term without a `/` that is not a top-level term names the one term that ends in it. Once
  `content/categories/snacks/chips/_index.md` exists, or a page writes `snacks/chips`,
  `categories = ["chips"]` means `snacks/chips`. When several terms end in it, fugo warns
  (`taxonomy-ambiguous-term`) and keeps the value top-level: write the term's path.
- A page's own terms (`page.terms`) are the ones it names; term keys (`page.term.key`, the
  keys of `site.taxonomies.categories`) are paths below the taxonomy: `snacks/chips`.

Declare the tree once with term pages, and let pages name the last term only:

```text
content/categories/snacks/_index.md
content/categories/snacks/chips/_index.md
content/posts/lays.md                       categories = ["chips"]
```

Term URLs follow the tree (`/categories/snacks/chips/`). To keep them flat, so that moving a
term in the tree does not change its URL, give terms a [permalink](/content-management/urls/#permalinks)
made of the last segment of their key:

{{< code-toggle file=config >}}
[permalinks.term]
  categories = "/categories/:sections[last]/"
{{< /code-toggle >}}

The tree still drives `parent`, `ancestors` and `sections`. This keeps the URLs of a flat
taxonomy as they were when it becomes hierarchical (`:slug` would make them again from the
title: `Cheddar & Sour Cream` is `cheddar--sour-cream` as a key but `cheddar-sour-cream` as a
slug). When a term's URL changes, keep the old one working with `aliases` in its `_index.md`.
