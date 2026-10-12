---
title: Migrating from Go templates
linkTitle: Migrating
description: What carries over from a site built with Go templates, what changes, and how to move a site to fugo step by step.
weight: 100
---

fugo began as a fork of a static site generator written in Go and keeps its content model, so
most of a Go-template site carries over unchanged. The big difference is the template language:
fugo renders [Tera](https://keats.github.io/tera/), not Go templates.

## What carries over

- **Content**: Markdown and HTML files, front matter in TOML, YAML or JSON, page bundles,
  sections, taxonomies, menus, `cascade`, summaries, shortcode calls in content, multilingual
  content.
- **Configuration**: the same keys, environments, `_merge`, in one file.
- **Layout names**: the current ones — `home.html`, `single.html`, `_partials/`, `_shortcodes/`,
  `_markup/` — and their lookup order.
- **Data, i18n, assets and static files**, mounts and themes from `themes/` or `_vendor/`.
- **Output**: URLs, aliases, pagination, feeds, sitemaps and highlighted code as the Go
  implementation writes them.

## What changes

| Go-template site | fugo |
|---|---|
| Go templates | Tera templates: see [Converting templates](/migrating/templates/) |
| `.Site.Title`, `.Page.RelPermalink` | `site.title`, `page.rel_permalink` |
| A configuration file named after the generator | `config.toml` (or `config.yaml`, `config.yml`, `config.json`) is the configuration file |
| Settings from environment variables | Settings come from files and flags only; secrets that templates read go in the [`.env` file](/configuration/introduction/#the-env-file) |
| A configuration directory (`config/_default/`) | one configuration file: each file's settings go in its table (`params.toml` in `[params]`); a configuration directory is an error (see [No configuration directory](/configuration/introduction/#no-configuration-directory)) |
| Environment folders (`config/production/`) | `[environments.production]` in the configuration file (see [Environments](/configuration/introduction/#environments)); a folder is an error |
| `--config a.toml,b.toml`, `--configDir` | one file: `--config` names one, and there is no `--configDir` |
| A template object named after the generator | fugo's `build` object holds the version and environment (`build.version`, `build.is_production`) |
| A build stats file for Tailwind CSS (`buildStats`) | none: [Tailwind's CLI](/asset-pipelines/tailwind-css/) finds the classes itself |
| The generator's build and server commands | `fugo` (or `fugo build`) / `fugo server` |
| Older layout names (`_default/`, `partials/`) | refused, with the new name |

The other differences — unsupported features and commands — are listed in
[Differences](/migrating/differences/).

## Moving a site

1. **Make one configuration file**, `config.toml`. Rename the generator's configuration file,
   and move the files of a configuration directory into it, each under its table:
   `config/_default/params.toml` becomes `[params]`, `config/_default/menus.en.toml`
   `[languages.en.menus]`, and an environment's folder (`config/production/`) its
   `[environments.production]` table. fugo stops with an error that lists each file of a
   configuration directory and its table. Settings your CI passes as environment variables move to
   `[environments.<name>]` or to command-line flags; `--environment` chooses the environment
   (no variable does), and secrets that templates read go in the
   [`.env` files](/configuration/introduction/#the-env-file).
2. **Use the current layout names.** If your layouts still use `_default/` or `partials/`,
   rename them; fugo's error message names the new path of each file.
3. **Convert the templates** to Tera, file by file, with the
   [conversion guide](/migrating/templates/). A theme needs the same.
4. **Check them**: `fugo templates check` reports syntax errors, unknown functions, missing
   context names and pages no template renders, without building.
5. **Build and compare**: `fugo build -d public-fugo` and compare with the `public/` of your
   previous build (`diff -r`). Differences in minified bytes and Sass output are expected; see
   [Differences](/migrating/differences/).

Content does not change, so content writers can keep working while the templates move.
