---
title: Content organization
description: How the content directory becomes pages, sections and URLs, and how page bundles keep a page together with its images and files.
weight: 10
---

## Pages and sections

Every Markdown (`.md`) or HTML (`.html`) file below `content/` is a page. Its path is its URL:

```text
content/
├── _index.md            → /             the home page
├── about.md             → /about/
└── posts/
    ├── _index.md        → /posts/        the section "posts"
    ├── first-post.md    → /posts/first-post/
    └── second-post.md   → /posts/second-post/
```

A directory directly below `content/` is a **section**, with or without an `_index.md`. A
deeper directory is a section only when it has an `_index.md`; the `_index.md` holds the
section's own content and front matter. Pages are of a *kind*: `home`, `section`, `page`
(a regular page), and `taxonomy` and `term` for [taxonomies](/content-management/taxonomies/).

Templates are picked by kind and section: a regular page of `posts` uses
`layouts/posts/single.html` if it exists, else `layouts/single.html`. See the
[template lookup order](/templates/lookup-order/).

## Page bundles

A page can be a directory: the page's content and the files that belong to it, such as
images, side by side.

```text
content/posts/
├── my-trip/             a leaf bundle: the page /posts/my-trip/
│   ├── index.md
│   ├── beach.jpg
│   └── map.gpx
└── archive/             a branch bundle: the section /posts/archive/
    ├── _index.md
    ├── banner.png
    └── old-post.md
```

Leaf bundle
: A directory with an `index.md` (or `index.html`). It is one regular page, and every other
  file in the directory and its subdirectories is a [page resource](/content-management/page-resources/)
  of that page: `beach.jpg` is published at `/posts/my-trip/beach.jpg` and processed with
  `page.resources | get_resource(name="beach.jpg") | resize(width=800)`.

Branch bundle
: A directory with an `_index.md`: a section. Its files that are not content are resources of
  the section page; its content files are pages of the section.

Keep a page's images in its bundle: links like `![Beach](beach.jpg)` in `index.md` resolve to
the resource, and moving the bundle moves everything together.

## URLs

The URL of a page follows its path, lower-cased, with spaces replaced by `-`. Change it with
front matter (`slug`, `url`) or with [permalinks](/content-management/urls/) per section.

## Headless pages

A page that should exist only to be read by templates — a block of text for the home page, a
bundle of images for a gallery — sets [build options](/content-management/build-options/):
`build: {render: never, list: never}`. It is not published and not listed, but
`get_page(path="/home-blocks/hero")` finds it.
