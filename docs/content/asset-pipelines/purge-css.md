---
title: Unused CSS
description: Ship each page only the CSS rules it uses, with fugo's built-in purge_css — no PostCSS or PurgeCSS needed.
weight: 40
---

`purge_css` removes the rules a page does not use. Unlike PurgeCSS, it works per page: each
published page gets the rules for its own elements, inlined.

```html {title="layouts/_partials/head.html"}
{% set css = get_asset(path="scss/main.scss") | to_css %}
{% set js = get_asset(path="js/main.js") | js_build %}
<style>{{ css | purge_css(content=[js]) }}</style>
```

`purge_css` prints a placeholder; when a page is written, fugo replaces it with the rules whose
selectors match the page's tags, classes and ids, and the words of its `<script>` elements. The
CSS is printed compactly for the project's browserslist targets.

`content`
: Resources or strings (such as scripts that add classes) whose words count as used on every
  page.

`safelist`
: Names, or `/regex/` patterns, that count as used.

`greedy`
: Keep any selector containing the string or matching the `/regex/`.

`blocklist`
: Names whose selectors are dropped even when used.

`variables`
: `true` drops custom properties nothing kept references.

`important`
: `false` drops `!important`.

Use it for sites with small pages and a large stylesheet. A site whose pages share most rules is
better served by one cached stylesheet.
