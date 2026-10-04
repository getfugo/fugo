---
title: Editing in the browser
description: Give people an editor at /admin/ — they sign in with Google or another account, edit pages, translations and images, save drafts and publish them — without access to the git repository.
weight: 200
---

With a `[cms]` table in its configuration, a build adds an editor to the site: a page at
`/admin/` where people edit pages, translations and images in a form, save drafts, and publish
them. They sign in with Google (or GitHub, Microsoft, a one-time code by email, …) through
[Cloudflare Access](https://developers.cloudflare.com/cloudflare-one/policies/access/), and
they never get access to the git repository: the editor's API commits as one bot account, with
each person as the author of their changes. The site's usual build then publishes them.

The editor needs no server of your own. It is static files, and its API is a
[Cloudflare Worker](https://developers.cloudflare.com/workers/) that runs next to the site's
files — on the free plan, it runs only when someone uses the editor:

```text
example.org/*            the site: static files of public/ (no code runs)
example.org/admin/       the editor: static files the build writes
example.org/admin/api/*  its API: public/_worker.js, the Worker the build writes
```

The editor works with sites whose repository is on GitHub; other git hosts may follow.

## How it works

1. Someone opens `example.org/admin/`. Cloudflare Access asks them to sign in (with Google, for
   example) before anything is served, and lets in only the people its policy names.
2. The editor lists the site's pages by section. They open one, and edit its front matter in a
   form and its text with a preview.
3. *Save draft* sends the changes to the API. The Worker checks the sign-in token Cloudflare
   Access added, finds the person's roles, checks every changed file against them, and commits
   to the page's draft branch as the bot, with the person as author.
4. Someone whose role may publish opens the draft, reviews the changes and clicks *Publish*.
   The Worker copies the draft onto your branch as one commit and deletes the draft.
5. The push to the branch runs your build and deploy (GitHub Actions, for example), as any other
   commit does.

## Set it up

### 1. Turn the editor on for production builds

Put `[cms]` in `config/production/cms.toml` (see [environments](/configuration/introduction/#environments)),
so production builds have the editor and `fugo server` does not:

```toml {title="config/production/cms.toml"}
media = "static/images/uploads"     # where uploads go when a page has no bundle (optional)

[git]
repo = "you/site"                   # GitHub owner/name
branch = "main"

[login]
team = "your-team"                  # https://your-team.cloudflareaccess.com
aud = "4714c1358e5d…"               # the Access application's audience (AUD) tag

[roles.writer]
edit = ["content/**/index.en.md", "content/**/*.{jpg,png,webp}"]

[roles.translator]
edit = ["content/**/index.th.md"]

[roles.publisher]
edit = ["**"]
publish = true
```

A file named `cms.toml` holds the table's keys; in `config.toml` the same settings go under
`[cms]`, `[cms.git]`, `[cms.login]` and `[cms.roles.writer]`. Every setting is described in
[CMS editor settings](/configuration/cms/).

`fugo build` (environment `production`) then writes the editor into `public/admin/`, and the API
into `public/_worker.js`, with the content index inside it (the titles and paths of every page,
drafts included: the API gives it to signed-in people only). `public/.assetsignore` keeps the
API's code out of the files Cloudflare serves, and `public/_headers` forbids framing the editor.

### 2. Deploy on Cloudflare Workers

Serve the site and the API from one Worker with
[static assets](https://developers.cloudflare.com/workers/static-assets/):

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
    "run_worker_first": ["/admin/api/*"]
  }
}
```

`run_worker_first` runs the Worker for the API only; every other request is a static file,
which costs nothing and does not count against the free plan's daily requests. The site lives
at its own domain only: Cloudflare Access protects that domain, not the Worker's `workers.dev`
or preview addresses, so keep those off (or put them behind Access too). Build and deploy with
[Cloudflare Workers](/host-and-deploy/cloudflare-workers/).

### 3. Sign in with Google

In the Cloudflare dashboard, under **Zero Trust** (free for up to 50 users):

1. **Settings → Authentication**: add Google as a login method. Cloudflare's guide shows how to
   create the OAuth client in the Google Cloud console; its redirect URI is
   `https://<team>.cloudflareaccess.com/cdn-cgi/access/callback`. Add other methods too if you
   like: GitHub, Microsoft, a one-time PIN by email for people without such an account.
2. **Access → Applications**: add a *self-hosted* application for `example.org/admin` with a
   policy that allows the people who edit (their emails, or an email domain).
3. Copy the application's **AUD tag** into `aud`, and your team name into `team`.

Access then signs people in before they reach `/admin/` and gives every request a signed token.
The Worker checks that token itself — its signature, team and audience — so a request that
does not come through Access (for example at the Worker's `workers.dev` address) is refused.

### 4. A bot account for GitHub

The Worker commits with one credential, stored as a Worker secret; editors need no GitHub
account.

- **A GitHub App** (recommended: not tied to a person, and its tokens expire after an hour).
  Create an app with the repository permission *Contents: Read and write* and nothing else,
  install it on the repository only, and generate a private key:

  ```sh
  npx wrangler secret put CMS_GITHUB_APP_ID      # the app's ID
  npx wrangler secret put CMS_GITHUB_APP_KEY     # the .pem file's contents
  ```

- **A fine-grained personal access token**, for the repository only, with *Contents: Read and
  write*:

  ```sh
  npx wrangler secret put CMS_GITHUB_TOKEN
  ```

Without the *Workflows* permission, GitHub itself refuses any change to `.github/workflows/`.
If the branch is protected (pull requests required), let the app bypass the rule, or the
editor cannot publish.

### 5. Say who has which role

The `CMS_USERS` secret maps emails to roles; a key starting with `@` covers an email domain:

```sh
npx wrangler secret put CMS_USERS
```

```json
{
  "ann@gmail.com": ["writer"],
  "somchai@gmail.com": ["translator"],
  "you@gmail.com": ["publisher"],
  "@example.org": ["writer"]
}
```

The list lives in Cloudflare, not in the repository: changing it needs no commit, editors'
emails are not published with the source, and no one can change it through the editor. The
Access policy says who may sign in at all; `CMS_USERS` says what each of them may do. Someone
who may sign in but has no role sees an error.

### 6. Build and deploy on every push

Every commit the editor makes is a push, so the build that runs on pushes publishes the
changes. With GitHub Actions:

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

## Roles: who may change what

A role's `edit` globs say which files it may create, change and delete, as paths in the
project; `publish = true` lets it publish drafts. Someone with several roles may do what any of
them may. In the globs, `*` matches any part of a name, `**` any part of a path (across
directories), `?` one character, `{a,b}` either alternative and `[abc]` one of the characters;
case matters.

Whatever a role says, the editor writes only these files:

| Where | Which files |
|---|---|
| The content directories | Markdown (and other content formats; HTML pages only with `html = true`), and its page bundles' images, documents, audio and video |
| `data/` and `i18n/` | TOML, YAML, JSON, CSV, XML |
| `config/_default/params.*`, `config/_default/menus.*` | TOML, YAML, JSON |
| The `media` directory | images, documents, audio and video |

Never `config.toml` or the rest of the configuration (a mount could publish your `.env`),
layouts, assets, content adapters (`_content.*` are templates), workflows, `wrangler.jsonc` or
dotfiles; and never SVG, HTML, CSS or JavaScript uploads, which would run on the site's
address. Site settings that editors should change belong in `config/_default/params.toml` and
the menus files, which the [configuration directory](/configuration/introduction/#configuration-directory)
merges like any other.

A role whose globs cannot match any of these files is an error at build time. Examples:

```toml
[roles.blogger]                  # the blog, and nothing else
edit = ["content/blog/**"]

[roles.translator]               # Thai pages of a site with index.th.md files
edit = ["content/**/*.th.md"]

[roles.settings]                 # the site's params and menus
edit = ["config/_default/params.toml", "config/_default/menus.*.toml"]

[roles.owner]                    # everything the editor may write; publishes
edit = ["**"]
publish = true
```

## Drafts and publishing

With `workflow = "review"` (the default), a save goes to the page's draft branch,
`cms/<page>-<hash>`, made from your branch on the first save; later saves of the page — by anyone
— add to it, so a writer and a translator work on one draft. The editor shows which pages have
drafts, and the *Drafts* list shows each draft's changes as a diff.

*Publish* (for roles with `publish = true`) checks every file of the draft against the
publisher's roles, then writes the draft's files onto your branch as one commit: its author is
the draft's first author, the others are `Co-authored-by`, and the publisher is
`CMS-Published-By`. If your branch changed one of the draft's files since the draft was made,
publishing stops and names the files: open the page, redo the change, and discard the draft.
*Discard* deletes a draft: anyone may discard their own, publishers any.

With `workflow = "direct"`, every save is a commit to your branch, and there are no drafts.

A save that changes a file someone else changed since you opened it is refused too; reload the
page and redo your change.

## The editor

- **Pages by section**, with their languages and drafts, and a filter.
- **A form for the front matter**: text, numbers, yes/no, dates, lists, nested tables and lists
  of tables, from the values the page has. Taxonomy fields (`tags`, `categories`, …) suggest the
  terms the site uses, which keeps spellings consistent. *Add a field* offers the keys other
  pages of the section use. [Field settings](/configuration/cms/#fields) choose labels, help
  text, choice lists and image pickers.
- **The text**, in Markdown, with a preview (without shortcodes or the site's styles).
- **Edit as text**: the whole file, front matter included, for anything the form does not show.
- **Translations**: a tab per language, and *+ language* to add one (it starts as a copy).
- **Files of the page**: a bundle's images and other files, uploads and deletions.
- **New pages**, written the way the section's pages are: as bundles or single files, with the
  section's front matter format and language suffixes.

Saving changes only what was edited. A file nobody changed is not written; in YAML front matter,
the keys that did not change keep their text, comments and formatting. TOML and JSON front
matter are written anew when they change.

## Try it locally

Build for production, then run the Worker with `wrangler dev`:

```sh
fugo build -e production
npx wrangler dev
```

There is no Cloudflare Access locally, so put the person to sign in as into `.dev.vars` (keep
it out of git), with the other secrets:

```sh {title=".dev.vars"}
CMS_DEV_USER=you@gmail.com
CMS_USERS='{"you@gmail.com": ["publisher"]}'
CMS_GITHUB_TOKEN=github_pat_…
```

`CMS_DEV_USER` signs in requests to `localhost` only, and only when it is set; deployed Workers
ignore it. The editor commits to the real repository: point `branch` at a test branch while you
try it (in a `config/staging/cms.toml` built with `-e staging`, for example).

## Limits

- At most 30 files per save, and uploads up to `maxUpload` (10 MB by default, 25 MB at most:
  the largest static file Cloudflare serves).
- On the Workers free plan, a request may use 10 ms of CPU time: large uploads may exceed it.
  The paid plan raises the limit.
- Pages the build makes from content adapters are not files, so the editor does not list them.
- The draft's preview is the Markdown text alone. For the whole page, build the draft branch in
  CI and upload it as a preview version (`npx wrangler versions upload --preview-alias <draft>`):
  that needs `preview_urls`, and Cloudflare Access on the preview URLs (the Worker's settings),
  or anyone can read the drafts there.

## Security notes

- **The editor is on the site's own address.** A script that runs on the site runs with the
  rights of whoever is signed in and opens that page: it could publish drafts. Scripts get into
  pages through HTML: `.html` content files (which the editor writes only with `html = true`),
  HTML in Markdown when `[markup.goldmark.renderer] unsafe = true`, and front matter or params
  that a template marks `safe`. With the review workflow, nothing reaches the site before a
  publisher reads the diff; with `workflow = "direct"`, give roles only to people you trust with
  the site.
- The API checks the Cloudflare Access token of every request itself, refuses requests from
  other sites (it checks the `Origin` header), and answers only JSON. The editor's pages may not
  be framed, and its preview runs in a sandboxed frame without scripts.
- The editor never writes the files that run code at build time or deploy time (configuration,
  layouts, assets, content adapters, workflows), whatever a role says, and matches their names
  the way the build does (`_Content.HTML` is a content adapter too).
- Commits carry the editor's email as author: in a public repository, the emails are public.

## Moving from Decap CMS

The editor reads and writes the same files Decap CMS does, so content needs no change:

1. Add `[cms]` and deploy as above.
2. Delete Decap's `static/admin/` (its `config.yml`, `index.html` and script) and its OAuth
   service.
3. Remove the editors' write access to the repository: they sign in through Access now.

The build warns while `static/admin/index.html` still exists: the editor replaces it.
