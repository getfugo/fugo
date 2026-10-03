---
title: Asset pipelines
description: Compile Sass, bundle JavaScript, fingerprint, minify and purge CSS — from templates, at build time, in process.
weight: 50
---

Files in `assets/` are not copied to the site. Templates get them with `get_asset`, transform
them with filters, and publish the results they link:

```html
{% set css = get_asset(path="scss/main.scss")
  | to_css(options={"outputStyle": "compressed"})
  | fingerprint %}
<link rel="stylesheet" href="{{ css.rel_permalink }}" integrity="{{ css.data.integrity }}">
```

A resource is published when a template reads its `rel_permalink` or `permalink`, and only
then. Results are cached for the build and, for images and remote files, on disk between builds.

| Step | Filter | Runs |
|---|---|---|
| [Sass](/asset-pipelines/sass/) | `to_css` | in process |
| [JavaScript bundling](/asset-pipelines/js-build/) | `js_build` | in process (rolldown) |
| [Fingerprinting and minification](/asset-pipelines/fingerprint-minify/) | `fingerprint`, `minify` | in process |
| [Unused CSS](/asset-pipelines/purge-css/) | `purge_css` | in process |
| [Bundling and generated files](/asset-pipelines/concat-and-templates/) | `concat_assets`, `asset_from_string`, `execute_as_template` | in process |
| [Post-processing](/asset-pipelines/post-processing/) | `post_process`, `defer` | after every page |

fugo runs no Node.js tools. To use [Tailwind CSS](/asset-pipelines/tailwind-css/), run its CLI
next to fugo. Images have their own page: [Image processing](/content-management/image-processing/).

## Finding assets

`get_asset(path="js/main.js")`
: One asset, or none.

`find_asset(pattern="css/*.css")`, `find_assets(pattern="icons/**.svg")`
: The first or all assets matching a glob.

Themes and modules can ship assets; mounts can add more directories under `assets/` (this
documentation mounts the repository's `brand/` directory as `assets/brand`). See
[Modules](/configuration/module/).
