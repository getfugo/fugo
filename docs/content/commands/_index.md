---
title: Commands
description: The fugo command line — build, server, templates check, config and version — with every option.
weight: 70
---

```sh
fugo [command] [options]
```

Run `fugo` with no command to build the site in the current directory. Every command takes the
global options (`--source`, `--environment`, `--config`, `--clock`, …), and options may come
before or after the command: `fugo -s site server` is `fugo server -s site`.

Options are spelled in kebab case (`--base-url`). The Go implementation's camelCase spellings
(`--baseURL`, `--buildDrafts`) are accepted too, so existing scripts keep working. A boolean option also takes
an explicit value: `-D=false` turns off `buildDrafts = true` from the configuration.

fugo exits with 0 on success, 1 when the build or the check fails, and 2 on a usage error.

The pages below are generated from fugo's own command-line definitions.
