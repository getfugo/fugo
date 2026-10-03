---
title: Markdown
description: The Markdown fugo renders — CommonMark with GitHub's extensions, attributes, alerts, definition lists and more — and how to configure it.
weight: 105
---

fugo renders Markdown the way goldmark, the Go implementation's default renderer, does:
CommonMark, plus these extensions, all on by default.

Tables
: GitHub's pipe tables, with alignment.

Strikethrough
: `~~deleted~~`.

Task lists
: `- [x] done` and `- [ ] to do`.

Footnotes
: `Text[^1]` and `[^1]: The note.`

Definition lists
: A term on its own line, then `: definition` (like this list).

Linkify
: Bare URLs become links.

Typographer
: `"quotes"`, `--` and `...` become typographic quotes, dashes and an ellipsis.

## Attributes

Attributes after a heading, and (with `parser.attribute.block`) on the line after a block:

```md
## Install {#install .wide}

A paragraph with a class.
{.lead}
```

## Alerts

GitHub's alert syntax renders through the blockquote [render hook](/templates/render-hooks/):

```md
> [!WARNING]
> Back up your data first.
```

The hook receives `alert_type` (`note`, `tip`, `important`, `warning`, `caution`) and can draw
the alert as it likes; this documentation draws them as callouts.

## Raw HTML

HTML in Markdown is omitted unless `markup.goldmark.renderer.unsafe = true`. Content files
ending in `.html` are HTML and are not converted.

## Configuration

{{< code-toggle file=config >}}
[markup.goldmark.extensions]
  definitionList = true
  footnote = true
  linkify = true
  strikethrough = true
  table = true
  taskList = true
  [markup.goldmark.extensions.typographer]
    disable = false
[markup.goldmark.parser]
  autoHeadingID = true
  autoHeadingIDType = "github"
  [markup.goldmark.parser.attribute]
    block = false
    title = true
[markup.goldmark.renderer]
  hardWraps = false
  unsafe = false
{{< /code-toggle >}}

`markup.goldmark.extensions.passthrough` keeps [math](/content-management/mathematics/) intact
for a render hook. `enableEmoji = true` turns `:smile:` into emoji.

The `extras` (insert, mark, subscript, superscript) and `cjk` settings are accepted but not
yet applied.

Only Markdown and HTML content is supported: AsciiDoc, Pandoc, reStructuredText and Org mode
files are not rendered.
