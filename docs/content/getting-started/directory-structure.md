---
title: Directory structure
description: What each directory of a fugo project holds, and how themes and mounts combine them.
weight: 20
---

A fugo project looks like this:

```text
my-site/
├── config.toml        the configuration (or config.yaml, config.yml, config.json)
├── config/            optional: configuration split by environment
├── content/           Markdown and HTML content
├── layouts/           Tera templates
├── assets/            files processed by asset pipelines (Sass, JavaScript, images)
├── static/            files copied to the site as they are
├── data/              data files: JSON, TOML, YAML, CSV, XML
├── i18n/              translation tables
├── themes/            themes, each a project of its own
└── public/            the built site (created by fugo)
```

Only `config.toml` and the content are required to start. A directory that does not exist is
simply empty.

`config.toml`, `config.yaml`, `config.yml`, `config.json`
: The [configuration](/configuration/). fugo reads the first one that exists, in that order.
  Other file names are not read: rename a configuration file named after another generator to
  `config.toml`.

`config/`
: Configuration split into files and [environments](/configuration/introduction/#configuration-directory):
  `config/_default/` for every build, `config/production/` and `config/development/` on top.

`content/`
: The pages of the site. The directory tree becomes the URL tree: `content/posts/hello.md` is
  published at `/posts/hello/`. See [Content organization](/content-management/organization/).

`layouts/`
: [Templates](/templates/), named after what they render: `baseof.html`, `home.html`,
  `single.html`, `list.html`, plus `_partials/`, `_shortcodes/` and `_markup/` (render hooks).

`assets/`
: Files that templates read with `get_asset` and process: Sass, JavaScript, images. Only the
  files a template publishes end up in the site. See [Asset pipelines](/asset-pipelines/).

`static/`
: Files copied to the root of the site unchanged: `static/favicon.ico` becomes `/favicon.ico`.

`data/`
: [Data files](/content-management/data-sources/) available in templates as `site.data`:
  `data/authors.toml` is `site.data.authors`.

`i18n/`
: [Translation tables](/content-management/multilingual/#translate-strings) for `i18n(key=…)`.

`themes/`
: [Themes](/configuration/themes/). A theme has the same structure as a project; the project's
  files win over the theme's.

`public/`
: Where `fugo` writes the site. Change it with `-d` or `publishDir`.

`package.json`, `npm.lock`, `node_modules/`
: The [npm packages](/asset-pipelines/npm-packages/) the site uses. fugo installs them into
  `node_modules` when it builds, and records their versions in `npm.lock`. Commit
  `package.json` and `npm.lock`, but not `node_modules`.

## Mounts

Each directory above is a *component*. [Module mounts](/configuration/module/) map any
directory of the project, or a theme, to a component: mount `node_modules/bootstrap/scss` into
`assets/bootstrap`, or a shared image directory into `static/images`. This documentation mounts
the repository's `brand/` directory into `assets/brand` to use the fugo logo.
