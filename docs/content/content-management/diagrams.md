---
title: Diagrams
description: Draw ASCII diagrams that fugo turns into SVG at build time, or render Mermaid diagrams in the browser with a code block render hook.
weight: 130
---

## GoAT diagrams

A code block of the language `goat` becomes an SVG image, built by fugo's port of
[GoAT](https://github.com/bep/goat):

````md
```goat
  .---.      .------.      .------.
  | a +----->| fugo +----->| site |
  '---'      '------'      '------'
```
````

```goat
  .---.      .------.      .------.
  | a +----->| fugo +----->| site |
  '---'      '------'      '------'
```

Attributes set the size and the class: ```` ```goat {width=300 class="wide"} ````. The
embedded `render-codeblock-goat.html` hook draws them; override it to change the markup, or call
`diagrams_goat(text=…)` from your own template.

## Mermaid diagrams

Mermaid runs in the browser. Add a code block render hook for `mermaid` that keeps the source
and records that the page needs the script:

```html {title="layouts/_markup/render-codeblock-mermaid.html"}
<pre class="mermaid">{{ inner }}</pre>
{{- store_set(key="has_mermaid", value=true) -}}
```

Then load Mermaid on the pages that use it, at the end of `baseof.html`:

```html
{% if store_get(key="has_mermaid") %}
  <script type="module">
    import mermaid from "https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.esm.min.mjs";
    mermaid.initialize({ startOnLoad: true });
  </script>
{% endif %}
```
