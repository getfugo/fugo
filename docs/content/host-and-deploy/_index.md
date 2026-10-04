---
title: Host and deploy
description: Publish a fugo site on any static host — GitHub Pages, GitLab Pages, Netlify, Cloudflare Workers or Pages, or your own server.
weight: 90
---

A fugo site is a directory of static files. Build it, and upload `public/`:

```sh
fugo build --minify
```

Set `baseURL` to the site's address in `config.toml`, or pass it at build time:
`fugo build --base-url https://example.org/`. For a site in a subdirectory, include the path:
`https://user.github.io/project/`.

Hosts build sites on Linux machines without fugo installed, so the build first downloads a
release:

```sh
V=1.0.0
curl -sL https://github.com/getfugo/fugo/releases/download/v$V/fugo_${V}_linux-amd64.tar.gz | tar -xz fugo
./fugo build --minify
```

Pin the version, so that a new release does not change your site unexpectedly.

## Caching between builds

Keep `resources/_gen/` between builds (commit it, or use the host's build cache) so that images
are not processed again; see [Caching](/configuration/caching/).

## Your own server

Copy `public/` with `rsync`, `scp` or any upload tool:

```sh
fugo build --minify && rsync -avz --delete public/ user@example.org:/var/www/site/
```

Configure the server to serve `404.html` for missing pages and, for a multilingual site, to
serve `/<lang>/404.html` under each language.
