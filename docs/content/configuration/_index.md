---
title: Configuration
description: Configure a fugo site — the configuration file and directory, environments, the .env file, and every section of settings.
weight: 60
---

A site's settings live in `config.toml` (or `config.yaml`, `config.yml`, `config.json`) at the
project root, or in a `config/` directory. Its keys are those of Go-template sites, whose
settings mostly carry over; see [Introduction](/configuration/introduction/) for how they are loaded.

| Settings | Where they are described |
|---|---|
| `baseURL`, `title`, `params`, directories, URLs, kinds | [Site settings](/configuration/site/) |
| `[languages]` | [Multilingual sites](/content-management/multilingual/) |
| `[taxonomies]` | [Taxonomies](/content-management/taxonomies/) |
| `[menus]` | [Menus](/content-management/menus/) |
| `[permalinks]` | [URLs](/content-management/urls/#permalinks) |
| `[markup]` | [Markdown](/content-management/markdown/), [Syntax highlighting](/content-management/syntax-highlighting/), [Table of contents](/configuration/site/#table-of-contents) |
| `[outputs]`, `[outputFormats]`, `[mediaTypes]` | [Output formats](/templates/output-formats/) |
| `[pagination]` | [Pagination](/templates/pagination/) |
| `[sitemap]` | [Sitemaps](/templates/sitemap/) |
| `[related]` | [Related content](/content-management/related-content/) |
| `[frontmatter]`, `[cascade]` | [Front matter](/configuration/front-matter/) |
| `[imaging]` | [Imaging](/configuration/imaging/) |
| `[caches]` | [Caching](/configuration/caching/) |
| `[module]`, `theme` | [Modules and mounts](/configuration/module/), [Themes](/configuration/themes/) |
| `[privacy]`, `[services]` | [Privacy and services](/configuration/privacy/) |
| `[security]` | [Security](/configuration/security/) |
| `[build]`, `[minify]` | [Build and minify](/configuration/build/) |

`fugo config` prints the configuration as fugo resolved it, defaults included.
