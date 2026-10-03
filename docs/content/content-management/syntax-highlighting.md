---
title: Syntax highlighting
description: Highlight code blocks at build time with fugo's port of Chroma, with line numbers, highlighted lines and inline styles or CSS classes.
weight: 120
---

fugo highlights code at build time with a port of [Chroma](https://github.com/alecthomas/chroma)
v2.19, the Go highlighter: the same lexers, styles and HTML. No JavaScript runs in the
browser.

## Code fences

Name the language after the opening fence, and add options in braces:

````md
```rust {linenos=inline, hl_lines=[2, "4-5"]}
fn main() {
    let name = "fugo";
    println!("hello");
    println!("{name}");
    println!("bye");
}
```
````

```rust {linenos=inline, hl_lines=[2, "4-5"]}
fn main() {
    let name = "fugo";
    println!("hello");
    println!("{name}");
    println!("bye");
}
```

`linenos`
: `true` or `table` (numbers in a table column), `inline`, `false`.

`linenostart`
: The first line number.

`hl_lines`
: Lines to highlight: `[2, "4-5"]` or `"2 4-5"`.

`anchorlinenos`, `lineanchors`
: Make the line numbers links, with ids prefixed by `lineanchors`.

`style`
: A Chroma style for this block (with inline styles).

`noClasses`
: `false` for class names, `true` for inline styles.

`hl_inline`
: Highlight without the `<pre>` wrapper.

## Configuration

{{< code-toggle file=config >}}
[markup.highlight]
  codeFences = true
  guessSyntax = false
  lineNos = false
  lineNumbersInTable = true
  noClasses = true
  style = "monokai"
  tabWidth = 4
{{< /code-toggle >}}

`noClasses = true` (the default) writes inline styles of `style`, so the page needs no CSS.
With `noClasses = false` fugo writes Chroma's class names (`<span class="k">`), and your
stylesheet colours them: this lets you switch colours with light and dark modes. Use any Chroma
style sheet; this documentation's theme defines its own colours in its Sass.

## In templates and shortcodes

The `highlight` filter highlights a string, and the embedded `highlight` shortcode highlights
inline content:

```html
{{ code | highlight(lang="toml", options="linenos=table") }}
```

```md
{{</* highlight go "linenos=inline" */>}}
package main
{{</* /highlight */>}}
```

A code block [render hook](/templates/render-hooks/) replaces the highlighter for every fence,
or for one language (`render-codeblock-mermaid.html`).
