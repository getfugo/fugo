---
title: Mathematics
description: Write LaTeX in Markdown and render it to MathML at build time with KaTeX, or in the browser.
weight: 140
---

fugo renders LaTeX at build time with [KaTeX](https://katex.org/) (0.16.22, with mhchem), as
`transform.ToMath` does in Go templates: pages get MathML and need no script.

## Keep the math intact

Enable the passthrough extension, so Markdown leaves the LaTeX alone and hands it to a render
hook:

{{< code-toggle file=config >}}
[markup.goldmark.extensions.passthrough]
  enable = true
  [markup.goldmark.extensions.passthrough.delimiters]
    block = [["\\[", "\\]"], ["$$", "$$"]]
    inline = [["\\(", "\\)"]]
{{< /code-toggle >}}

## Render it

```html {title="layouts/_markup/render-passthrough.html"}
{%- set opts = {"displayMode": type == "block"} -%}
{{- inner | to_math(options=opts) -}}
```

Now `\(E = mc^2\)` and

```md
$$
\int_0^1 x^2\,dx = \frac{1}{3}
$$
```

render as MathML. `to_math` takes KaTeX's options: `output` (`mathml`, the default, `html` or
`htmlAndMathml` — the last two need KaTeX's CSS), `displayMode`, `macros`, `throwOnError`,
`errorColor`, `strict` and more. A formula KaTeX rejects fails the build: set `throwOnError`
to false to show the error in the page instead, or call `to_math(optional=true)` to get a
warning and no output.

## In the browser instead

Leave the passthrough on, write the delimiters back in the hook (`\(` … `\)`), and load KaTeX's
or MathJax's auto-render script on the page.
