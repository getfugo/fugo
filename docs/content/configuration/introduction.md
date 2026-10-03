---
title: Introduction
description: How fugo finds and merges its configuration — the file, the configuration directory, environments and command-line flags — and the .env file for secrets.
weight: 10
---

## The configuration file

fugo reads the first of `config.toml`, `config.yaml`, `config.yml` and `config.json` in the
project directory (`--source`). If there are several, it warns about the others. `--config`
names other files, comma-separated; the first wins where they disagree.

```toml {title="config.toml"}
baseURL = "https://example.org/"
title = "My site"
languageCode = "en-US"

[params]
  description = "Notes on Rust and the web"
```

Keys are case-insensitive: `baseURL` and `baseurl` are the same. Other file names are not
read: rename a configuration file named after another generator to `config.toml`.

## Configuration directory

Settings can be split into files in `config/` (or `--config-dir`):

```text
config/
├── _default/
│   ├── config.toml       root settings
│   ├── params.toml       [params]
│   ├── menus.en.toml     [languages.en.menus]
│   └── languages.toml    [languages]
├── production/
│   └── config.toml       only in production
└── development/
    └── params.toml       only in development
```

A file's name says where its settings go: `params.toml` is `[params]`, `menus.en.toml` the
English menus, `config.toml` the root.

## Environments

The environment is `production` for `fugo build` and `development` for `fugo server`;
`--environment staging` chooses another. `config/<environment>/` is merged over
`config/_default/`, `.env.<environment>` over `.env` (see [The .env file](#the-env-file)), and
templates read it as `build.environment`, `build.is_production`, `build.is_development`.

## Precedence

From lowest to highest: themes (see [Themes](/configuration/themes/#configuration)), the configuration
file, `config/_default/`, `config/<environment>/`, command-line flags (`--base-url`,
`--destination`, …).

A table with `_merge = "none"` replaces the table below it instead of merging into it.

## Environment variables

Settings never come from environment variables: a setting that differs per machine or per
deployment belongs in `config/<environment>/` or on the command line, and the environment
itself comes from the command (`--environment`). Keep API keys and other secrets in the
`.env` files.

## The .env file

A `.env` file in the project directory holds values that templates read with
[`get_env`](/reference/functions/system/get_env/) and that must not be committed, such as API keys.
`.env.<environment>` (`.env.production`, `.env.development`, …) is read after it, and its
lines win: put what differs per environment there. Add them to `.gitignore` (`.env` and
`.env.*`):

```sh {title=".env"}
# YouTube Data API key (never commit this file)
YOUTUBE_API_KEY=your-key
```

```jinja
{% set key = get_env(name="YOUTUBE_API_KEY") %}
{% if key %}
  {% set video = get_remote(url=api_url, options={"headers": {"X-Goog-Api-Key": key}}, optional=true) %}
{% endif %}
```

- fugo reads them for every build; `fugo server` reloads when one changes.
- Its names need no [`security.funcs.getenv`](/configuration/security/) entry. An environment
  variable of the same name wins, so CI can set the secret either way.
- They are not configuration: `fugo config` does not print them, `site.params` does not see
  them, and `FUGO_` names in them are ignored with a warning (`env-file-prefix`; set those in
  the environment).
- One `NAME=value` per line; `#` starts a comment, `export` in front is accepted, and a value may
  be quoted (`'as written'`, or `"with \n escapes"`). Values are one line, and `${NAME}` is not
  expanded.
