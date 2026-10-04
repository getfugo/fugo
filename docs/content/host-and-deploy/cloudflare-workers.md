---
title: Cloudflare Workers
description: Publish a fugo site as a Cloudflare Worker with static assets — free for static files — with the browser editor's API in the same Worker.
weight: 35
---

A Worker with [static assets](https://developers.cloudflare.com/workers/static-assets/) serves
the files of `public/`; requests for static files are free and unlimited. Cloudflare suggests
Workers rather than Pages for new sites, and the [browser editor](/content-management/cms/)
needs them: its API runs in the same Worker.

Describe the Worker next to `config.toml`:

```jsonc {title="wrangler.jsonc"}
{
  "name": "site",
  "compatibility_date": "2026-10-01",
  "assets": {
    "directory": "public",
    "not_found_handling": "404-page"
  }
}
```

and deploy after each build:

```sh
fugo build --minify
npx wrangler deploy
```

Add a custom domain in the Worker's settings (or `"routes"` in `wrangler.jsonc`) and set
`baseURL` to it.

## With the browser editor

A build with [`[cms]`](/configuration/cms/) also writes `public/_worker.js` (the editor's API),
`public/.assetsignore` (which keeps `_worker.js` out of the static files) and a rule in
`public/_headers` (Cloudflare's headers file: the editor may not be framed). Make the API the
Worker's code, run it for the API's paths only, and serve the site at its own domain only,
where Cloudflare Access protects the editor:

```jsonc {title="wrangler.jsonc"}
{
  "name": "site",
  "main": "public/_worker.js",
  "compatibility_date": "2026-10-01",
  "routes": [{ "pattern": "example.org", "custom_domain": true }],
  "workers_dev": false,
  "preview_urls": false,
  "assets": {
    "directory": "public",
    "not_found_handling": "404-page",
    "run_worker_first": ["/admin/api/*"]
  }
}
```

The API's secrets (`CMS_USERS` and the GitHub credential) are set once with
`npx wrangler secret put`; deploys keep them.

## Deploy from GitHub Actions

Create an API token with the *Edit Cloudflare Workers* template and store it as the repository
secret `CLOUDFLARE_API_TOKEN`:

```yaml {title=".github/workflows/deploy.yml"}
on:
  push:
    branches: [main]
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: |
          V=1.1.0
          curl -sL https://github.com/getfugo/fugo/releases/download/v$V/fugo_${V}_linux-amd64.tar.gz | tar -xz fugo
          ./fugo build --minify
      - uses: cloudflare/wrangler-action@v3
        with:
          apiToken: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          command: deploy
```

A file may be at most 25 MiB, and a deploy at most 20,000 files on the free plan (100,000 on
the paid plan).
