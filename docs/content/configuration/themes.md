---
title: Themes
description: Use a theme, override its templates and assets, and how a theme's configuration merges with yours.
weight: 70
---

A theme is a project whose layouts, assets, content, data and i18n files are used under yours.

{{< code-toggle file=config >}}
theme = "my-theme"
{{< /code-toggle >}}

fugo looks for `themes/my-theme` (or `themesDir`). `theme` may be a list: the first wins where
themes provide the same file. A theme can import other themes with `[[module.imports]]`.

Themes for fugo are written in Tera, with the current layout names; a theme written in Go
templates must be converted first (see [Migrating from Go templates](/migrating/)).

## Overriding a theme

Any file of yours with the same path wins: `layouts/_partials/footer.html` replaces the theme's
footer, `assets/css/theme.css` its stylesheet, `i18n/en.toml` keys add to or replace its
strings. A theme's more specific template still beats your general one: the theme's
`layouts/posts/single.html` wins over your `layouts/single.html` for posts.

## Configuration

A theme's `config.toml` (or `config/` directory) is merged below yours: your values always win,
and the theme adds what you do not set.

| Your table | What the theme adds |
|---|---|
| `params` (also per language) | every key you lack, at every level |
| `menus`, `outputFormats`, `mediaTypes` | entries you lack |
| any other (`languages`, `outputs`, `taxonomies`, `markup`, …) | nothing |

Root values (`title`, `baseURL`) never come from a theme. Write `_merge = "deep"`,
`"shallow"` or `"none"` in a table to change what it takes; `_merge = "none"` at the root
ignores theme configuration altogether.
