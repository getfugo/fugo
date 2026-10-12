---
title: Introduction
description: How fugo reads its configuration — one file, its environments and command-line flags — and the .env file for secrets.
weight: 10
---

## The configuration file

The configuration is one file: `config.toml`, `config.yaml`, `config.yml` or `config.json` in
the project directory (`--source`). If there are several, fugo stops with an error that names
them: merge them into one. `--config` names another file instead (one file, not a list).

```toml {title="config.toml"}
baseURL = "https://example.org/"
title = "My site"
languageCode = "en-US"

[params]
  description = "Notes on Rust and the web"
```

Keys are case-insensitive: `baseURL` and `baseurl` are the same. Other file names are not
read: rename a configuration file named after another generator to `config.toml`.

## No configuration directory

Settings are not split into files: fugo does not read a configuration directory
(`config/_default/`, `config/production/`). When a folder of `config/` holds configuration
files, fugo stops with an error that names the table of the configuration file each one's
settings belong in:

```text
config/_default/params.toml       →  [params]
config/_default/menus.en.toml     →  [languages.en.menus]
config/_default/imaging.toml      →  [imaging]
config/_default/config.toml       →  the root
config/production/params.toml     →  [environments.production.params]
```

The settings of one environment go in its [`[environments.<name>]`](#environments) table.

## Environments

The environment is `production` for `fugo build` and `development` for `fugo server`;
`--environment staging` chooses another. The settings of one environment go in its table,
`[environments.<name>]`, which fugo merges over the rest of the configuration when it builds for
that environment:

```toml {title="config.toml"}
baseURL = "https://example.org/"
title = "My site"

[environments.production.minify]
minifyOutput = true

[environments.development]
buildDrafts = true
buildFuture = true

[environments.development.params]
analytics = false
```

The table holds any setting, as the root does: `[environments.production.params]` is `[params]`
in production builds. It is merged as a later file would be: tables merge key by key, other
values replace, and a table with `_merge = "none"` replaces the one it is merged into. The other
environments' tables are not read, and `fugo config` prints the result without them. A theme's
`[environments.<name>]` tables apply to the theme's own configuration.

`.env.<environment>` is read over `.env` (see [The .env file](#the-env-file)), and templates read
the environment as `build.environment`, `build.is_production`, `build.is_development`.

## Precedence

From lowest to highest: themes (see [Themes](/configuration/themes/#configuration)), the configuration
file, `[environments.<environment>]`, command-line flags (`--base-url`, `--destination`, …).

A table with `_merge = "none"` replaces the table below it instead of merging into it.

## Environment variables

Settings never come from environment variables: a setting that differs per machine or per
deployment belongs in `[environments.<name>]` or on the command line, and the environment
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
