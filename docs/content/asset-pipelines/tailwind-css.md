---
title: Tailwind CSS
description: Run Tailwind's own CLI next to fugo and publish the stylesheet it writes as an asset.
weight: 70
---

fugo does not run Tailwind CSS. Run Tailwind's CLI next to fugo (the
[standalone binary](https://tailwindcss.com/blog/standalone-cli), or `npx @tailwindcss/cli`), and
publish the stylesheet it writes like any other asset:

```css {title="assets/css/main.css"}
@import "tailwindcss";
```

```sh
tailwindcss -i assets/css/main.css -o assets/css/site.css --watch &
fugo server
```

Tailwind finds the classes in your layouts and content itself. It skips the files your
`.gitignore` lists, such as `public/`; name other places with `@source`.

```html {title="layouts/_partials/css.html"}
{%- set css = get_asset(path="css/site.css") | minify | fingerprint -%}
<link rel="stylesheet" href="{{ css.rel_permalink }}" integrity="{{ css.data.integrity }}">
```

When Tailwind rewrites `assets/css/site.css`, `fugo server` rebuilds and reloads the page. For a
release, run Tailwind once before the build:

```sh
tailwindcss -i assets/css/main.css -o assets/css/site.css
fugo build --minify
```

`minify` adds the vendor prefixes your [browserslist](/asset-pipelines/fingerprint-minify/) needs.
Tailwind writes only the classes it found, so [`purge_css`](/asset-pipelines/purge-css/) is
optional.
