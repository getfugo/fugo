---
title: GitHub Pages
description: Build and publish a fugo site on GitHub Pages with GitHub Actions.
weight: 10
---

In the repository's **Settings › Pages**, set the source to **GitHub Actions**, then add this
workflow:

```yaml {title=".github/workflows/pages.yml"}
name: Pages

on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: false

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install fugo
        run: |
          V=1.0.0
          curl -sL "https://github.com/getfugo/fugo/releases/download/v$V/fugo_${V}_linux-amd64.tar.gz" | tar -xz fugo
          sudo install fugo /usr/local/bin/
      - id: pages
        uses: actions/configure-pages@v5
      - name: Build
        run: fugo build --minify --base-url "${{ steps.pages.outputs.base_url }}/"
      - uses: actions/upload-pages-artifact@v3
        with:
          path: public

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

`configure-pages` gives the site's URL (`https://user.github.io/repository`, or your custom
domain), so the same workflow works for user, organization and project sites.

To keep processed images between runs, cache `resources/_gen` with `actions/cache`.
