---
title: Font subsetting
description: Cut the site's fonts down to the characters its pages and style sheets use, after the build, in process — an icon font from 150 KB to 3 KB.
weight: 45
---

`[fonts]` cuts the published fonts down to the characters the site uses, once every page is
written. An icon font such as Font Awesome, whose files hold some 2,000 icons, keeps the few
dozen the site shows, and goes from about 150 KB to 3 KB:

{{< code-toggle file=config >}}
[[fonts.subset]]
  paths = ["assets/webfonts/*"]
  from = "content"
{{< /code-toggle >}}

Each `[[fonts.subset]]` entry covers some fonts and says which characters they keep:

`paths`
: Globs of the fonts' paths in the published site, whether they are static files or
  resources (`*` stops at `/`, `**` does not). TrueType, OpenType, WOFF and WOFF2 fonts
  (`.ttf`, `.otf`, `.woff`, `.woff2`) are cut down; other files are left alone.

`from`
: Which characters count as used.
  - `text` (the default), for text fonts: the text of the pages (outside `<script>` and
    `<style>`, with the `alt`, `placeholder`, `title` and `value` attributes) and the
    characters of `content` strings.
  - `content`, for icon fonts: only the strings of the CSS `content` and `quotes`
    declarations (`.fa-star::before { content: "\f005" }`) and of the custom properties they
    use (`--fa: "\f005"`, which `content: var(--fa)` draws), in style sheets, `<style>`
    elements and `style` attributes. The string of a `url()` is an image, not text.

`keep`
: Characters kept anyway, such as those of text that only scripts add.

A font covered by several entries follows the first. An entry that covers no font is a warning.

## What a font keeps

The glyphs of its characters, those its layout features reach from them (ligatures,
alternates, accents) and its layout tables, cut down to those glyphs. Every layout feature is
kept, since a page may turn any of them on (`font-variant-caps`, `font-feature-settings`), and so
is the hinting. The subsetter is [klippa](https://github.com/googlefonts/fontations), a port of
HarfBuzz's.

A font is written back in its own format: TrueType or OpenType, WOFF, or WOFF2. Some fonts are
left as they are:

- A font none of whose characters the site uses (Font Awesome's `fa-v4compatibility`, say).
- A variable font whose layout tables the subsetter cannot cut down yet: it would lose its
  kerning, mark positioning or ligatures. The build warns.
- A WOFF2 font with CFF outlines, which fugo does not write yet. The build warns.

The build's summary line counts the fonts cut down and their sizes before and after. `fugo
server` cuts them down too.

A font is cut down after its URL is written: the name of a
[fingerprinted](/asset-pipelines/fingerprint-minify/) font follows the whole font, not what is
left of it, and its `integrity` does not match. Do not give a cut-down font an `integrity`
attribute, and do not cache it for longer than the pages that use it: when they start to use
another icon, the font changes but keeps its name.

## When to use it

For icon fonts, always: no page shows more than a few of their icons. For text fonts, when the
site's text uses few characters of a large font (a CJK or Thai font, a display face for
headings); a font a site serves to text written by its readers (comments) needs every
character they might type, which `keep` cannot list, so leave it out.
