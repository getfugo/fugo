---
title: Editing in the browser
description: Give people an editor at /admin/ — they sign in with Google or another account, edit pages, translations and images, save drafts and publish them — without access to the git repository.
weight: 200
---

With a `[cms]` table in its configuration, a build adds an editor to the site: a page at
`/admin/` where people edit pages, translations and images in a form, save drafts, and publish
them. They sign in with their GitHub or Google account, and they never get access to the git
repository: the editor's API commits as one bot account, with each person as the author of their
changes. The site's usual build then publishes them.

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

1. Someone opens `example.org/admin/` and signs in with GitHub or Google. The Worker lets in
   only the people the `CMS_USERS` secret gives a role, and keeps them signed in with a cookie
   it signs.
2. The editor lists the site's pages by section. They open one, and edit its front matter in a
   form and its text as rich text (or as Markdown), with a preview.
3. *Save draft* sends the changes to the API. The Worker checks the sign-in, finds the person's
   roles, checks every changed file against them, and commits
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
provider = ["github", "google"]     # or one of them

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
at its own domain only, the one people sign in at, so keep the Worker's `workers.dev` and
preview addresses off. Build and deploy with
[Cloudflare Workers](/host-and-deploy/cloudflare-workers/).

### 3. Sign in with GitHub or Google

The Worker signs people in itself, with the providers `provider` names. Each needs an OAuth app
of your own, which is free; people see its name when they sign in. Use one or both: GitHub for
people who have an account there, Google for everyone else.

**GitHub.** Under GitHub's **Settings → Developer settings → OAuth Apps** (yours, or an
organisation's), create an app:

- *Homepage URL*: `https://example.org`
- *Authorization callback URL*: `https://example.org/admin/api/callback/github`

Then *Generate a new client secret*, and store the app's client ID and the secret:

```sh
npx wrangler secret put CMS_GITHUB_CLIENT_ID
npx wrangler secret put CMS_GITHUB_CLIENT_SECRET
```

**Google.** In the [Google Cloud console](https://console.cloud.google.com/), open **Google Auth
Platform** (*APIs & Services → OAuth consent screen*) and set it up, with your site's name and
the audience *External*. Under **Clients**, create a client of the type *Web application*, with
the authorized redirect URI `https://example.org/admin/api/callback/google`. Under **Audience**,
publish the app: while it is in testing, only its test users may sign in. An app that asks for no
more than the email and the profile, like this one, needs no review by Google. Store its client
ID and secret:

```sh
npx wrangler secret put CMS_GOOGLE_CLIENT_ID
npx wrangler secret put CMS_GOOGLE_CLIENT_SECRET
```

**The session key**, which signs the cookie that keeps people signed in: any random text of 32
characters or more.

```sh
openssl rand -base64 32 | npx wrangler secret put CMS_SESSION_KEY
```

Someone who opens the editor without being signed in sees a button for each provider. Back from
the provider, the Worker reads the account's verified emails (on GitHub, all of them, the primary
one first) and signs them in with the first one that `CMS_USERS` gives a role: that email is the
author of their commits, with the account's name. An account with no such email is refused. They
stay signed in for seven days, or until *Sign out*; changing `CMS_SESSION_KEY` signs everyone
out. The Worker keeps nothing of the provider's: it reads the emails once and drops the
provider's token.

#### Or Cloudflare Access

[Cloudflare Access](https://developers.cloudflare.com/cloudflare-one/policies/access/) can sign
people in instead, before they reach the editor, with any of its login methods (a one-time PIN by
email, Microsoft, …). It is free for up to 50 users, and set up in the Cloudflare dashboard,
under **Zero Trust**:

1. **Settings → Authentication**: add the login methods. Cloudflare's guide shows how to create
   Google's OAuth client, for example; its redirect URI is
   `https://<team>.cloudflareaccess.com/cdn-cgi/access/callback`.
2. **Access → Applications**: add a *self-hosted* application for `example.org/admin` with a
   policy that allows the people who edit (their emails, or an email domain).
3. In `[login]`, set `provider = "cloudflare-access"`, the application's **AUD tag** as `aud`,
   and your team name as `team`. Access needs no `CMS_SESSION_KEY` or client secrets.

Access then signs people in before they reach `/admin/` and gives every request a signed token.
The Worker checks that token itself — its signature, team and audience — so a request that
does not come through Access (for example at the Worker's `workers.dev` address) is refused.

### 4. A bot account for GitHub

The Worker commits with one credential, stored as a Worker secret; editors need no GitHub
account.

- **A GitHub App** (recommended: not tied to a person, and its tokens expire after an hour).
  Create an app with the repository permissions *Contents: Read and write* and *Pull requests:
  Read and write* (for the drafts' [pull requests](#drafts-and-publishing); without it there are
  none) and nothing else, install it on the repository only, and generate a private key:

  ```sh
  npx wrangler secret put CMS_GITHUB_APP_ID      # the app's ID
  npx wrangler secret put CMS_GITHUB_APP_KEY     # the .pem file's contents
  ```

- **A fine-grained personal access token**, for the repository only, with *Contents: Read and
  write* and *Pull requests: Read and write*:

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
emails are not published with the source, and no one can change it through the editor. It says
who may sign in and what each of them may do; the Worker reads it on every request, so taking
someone out of it takes effect at once. (With Cloudflare Access, the Access policy says who may
sign in at all, and someone it lets in who has no role sees an error.)

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
          V=1.4.0
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
— add to it, so a writer and a translator work on one draft. A page with a draft opens as the
draft has it, and saving it adds to that draft. The editor shows which pages have
drafts, and the *Drafts* list shows each draft's changes as a diff.

The first save also opens a pull request for the draft, labelled `fugo-cms`, which links back to
the draft in the editor: the draft shows on GitHub too, for review and comments. Publishing or
discarding the draft deletes its branch, which closes the pull request; a published draft's pull
request says which commit published it. Without the *Pull requests* permission, drafts have no
pull request and work the same.

*Publish* (for roles with `publish = true`) checks every file of the draft against the
publisher's roles, then writes the draft's files onto your branch as one commit: its author is
the draft's first author, the others are `Co-authored-by`, and the publisher is
`CMS-Published-By`. If your branch changed one of the draft's files since the draft was made,
publishing first merges your branch into the draft, as git does: a file your branch moved keeps
the draft's changes, and changes to different lines of a file are both kept. Only when both
changed the same lines does publishing stop and name the files: open the page, redo the change,
and discard the draft. *Discard* deletes a draft: anyone may discard their own, publishers any.

With `workflow = "direct"`, every save is a commit to your branch, and there are no drafts.

A save that changes a file someone else changed since you opened it is refused too; reload the
page and redo your change.

## The editor

- **Pages by folder**, at any depth: a section lists its folders, then its pages, with their
  languages and drafts, and each folder lists its own the same way. The crumbs above a folder
  or a page lead back up to *Pages*, the top, whose folders are the sections. The filter finds
  pages and folders anywhere below the folder shown, and says where each is. The pages of a
  taxonomy's terms (`content/brands/lays/_index.md`) are listed as pages of its section
  (*Brands*, "3 brands"), even when it has no `_index` page of its own; in a hierarchical
  taxonomy, a term with terms below it is a folder.
- **A tree of folders** in the sidebar: the sections, with their folders below them at any
  depth, which open and close in place. The folder shown, or the one whose list has the page
  shown, is marked, and the folders above it open.
- **A form for the front matter**: text, numbers, yes/no, dates, lists, nested tables and lists
  of tables, with the labels and inputs the build works out from the site's content, and the
  values pages share suggested as you type. Taxonomy fields (`tags`, `categories`, …) suggest
  the terms the site uses, which keeps spellings consistent. *Add a field* offers the keys other
  pages of the section use, and a table's *Add…* the keys other pages have in it. A value that
  is the same in every language of the page (an image, a rating) is edited once for all of
  them, and new pages start with the values the section's pages share.
  [Field settings](/configuration/cms/#fields) change labels, help text and inputs, and add
  choice lists (one choice or several), defaults, number bounds, required values and closed
  tables with a summary.
- **The text**, as rich text or as Markdown: *Rich text* and *Markdown* above it switch between
  them, and the browser remembers the choice. Rich text shows the text as it reads, with a
  toolbar for paragraph styles (headings, quotes, code blocks), bold, italic, strikethrough,
  code, links, lists, images from the page's files and rules; Ctrl+B, Ctrl+I and Ctrl+K (a link;
  ⌘ on a Mac) work too, and Tab indents a list item. Pasted text keeps what Markdown can hold of it
  (headings, lists, bold, italic, links), not its fonts or colours. Raw HTML, shortcodes on lines
  of their own and link definitions show as their source, in a box where they are edited as
  text; the editor never runs them. A save writes only the blocks (paragraphs, lists, tables…)
  that were edited: the others keep their text exactly, with the blank lines between them.
- **A preview** of the text inside the site's own page: the published page of its language (for
  a new page, another page of its folder) with the edited text and front matter in place,
  updated as you type. The editor finds the element that holds a page's text and the elements
  that show a front matter value as published; where it cannot, mark them in the site's
  templates with `data-cms-body` and `data-cms-field="<key>"`. Shortcodes, scripts and files from
  other sites (fonts or styles from a CDN) are left out.
- **Edit as text**: the whole file, front matter included, for anything the form does not show.
- **Translations**: a tab per language, and *+ language* to add one (it starts as a copy).
- **Files of the page**: a bundle's images and other files, uploads and deletions.
- **New pages**: *New page* makes one in the folder shown, written the way its section's pages
  are: as bundles or single files, with the section's front matter format and language
  suffixes.
- **New folders**: *New folder* makes a folder in the folder shown, with an `_index` page that
  holds the folder's title and text (*Edit folder page*), and opens it, ready for its pages. The
  folder's page is saved with the first page saved in it, in the same draft, or on its own.
  *New section* (on the start page, or at the top) makes a folder at the top: a new section,
  whose pages are written like those of the largest section.
- **Moving a page** to another folder of its section, at any depth, with all its files
  (translations, images, everything in its bundle). Each language's file keeps the URL its page
  had in `aliases`, so that old links still work. A move saves the page like an edit: in a
  draft, with `workflow = "review"`.

Saving changes only what was edited. A file nobody changed is not written; in YAML front matter,
the keys that did not change keep their text, comments and formatting. TOML and JSON front
matter are written anew when they change.

## Try it locally

`fugo server` answers the editor's API itself, from the git repository on your computer,
whenever the configuration it serves has `[cms]`. You need no Worker, git host or sign-in:

```sh
fugo server -e production   # or put a cms.toml into config/development/ too
```

Then open `http://localhost:1313/admin/`. The editor works as it does on the site:

- **Who you are:** you edit as your git identity (`git config user.name` and `user.email`),
  with every role, so you may change anything the editor can and publish.
- **Drafts:** each page's draft is a local branch `cms/<id>`, made from the branch you have
  checked out (`branch` of `[cms.git]` is not used locally).
- **Publishing:** *Publish* (or, with `workflow = "direct"`, *Save and publish*) commits onto
  that branch and updates your files, and the server rebuilds the site. If you have uncommitted
  changes to the same files, it refuses until you commit or stash them; changes to other files
  stay as they are.
- **Nothing leaves your computer:** push with `git push` when you are happy, or delete the
  drafts' branches.

The API answers only at `localhost` (or `127.0.0.1`), and only to this computer, even with
`--bind 0.0.0.0`. Publishing a draft whose files changed on the branch in the meantime needs
git 2.38 or newer.

### Try the Worker

To try the Worker itself, with sign-in and the GitHub repository, build for production, then
run it with `wrangler dev`:

```sh
fugo build -e production
npx wrangler dev
```

Locally, a provider cannot send people back (its app knows the site's address only), so put the
person to sign in as into `.dev.vars` (keep it out of git), with the other secrets:

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
- Moving a page counts each of its files twice (its old and its new path), so a page with more
  than 15 files cannot be moved in the editor (move it in git).
- On the Workers free plan, a request may use 10 ms of CPU time: large uploads may exceed it.
  The paid plan raises the limit.
- Pages the build makes from content adapters are not files, so the editor does not list them.
- Rich text writes the blocks it changes in Markdown of its own: an edited paragraph keeps the
  emphasis, links and images that did not change as they were written, but its other text may
  gain backslashes (`\*`) where a character would otherwise start Markdown, and a list it
  rewrites uses the text's first bullet. Markdown it cannot show (footnotes, attributes such as
  `{#id}`, definition lists) stays as text, as written; edit it in Markdown if you prefer.
- The preview renders the text with the editor's Markdown, not the site's render hooks. For the
  page exactly as the site builds it, build the draft branch in
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
- The API checks the sign-in of every request itself (the session cookie's signature, or the
  Cloudflare Access token), refuses requests from other sites (it checks the `Origin` header),
  and answers only JSON. The session cookie is the site's host's only, sent over HTTPS only, and
  scripts cannot read it.
- `fugo server` answers the API without a sign-in, so it answers only connections from your
  computer to `localhost`: other computers on the network, and pages of other sites (directly,
  or through a name of theirs that leads to your computer), cannot use it.
- A `@domain` key in `CMS_USERS` lets in every account with a verified email of that domain,
  including accounts made while someone still had such an email. Name people one by one when
  that matters. The editor's pages may not
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
3. Remove the editors' write access to the repository: they sign in to the editor now.

The build warns while `static/admin/index.html` still exists: the editor replaces it.

The drafts Decap's editorial workflow left open — its branches `cms/<collection>/<slug>` and
their pull requests, labelled `decap-cms/draft` — are in the *Drafts* list too, under their pull
request's title: review, publish or discard them there. Publishing merges what the site moved or
changed since, as for any draft; but a page a Decap draft *creates* stays at the path it had then
(git follows only files that existed), so move it where it belongs before publishing.

Decap's `config.yml` mostly needs no counterpart: the build works out the fields from the
content (see [fields](/configuration/cms/#fields)). Its settings map to `[cms.fields]` like this:

| Decap | fugo |
|---|---|
| `collections` with `folder`, `create`, `nested` | the sections and their folders, found in the content |
| `widget: string`, `text`, `markdown`, `datetime`, `number`, `boolean`, `image`, `list`, `object`, `hidden` | worked out from the values; `widget` for another |
| `widget: select` with `options`, `multiple` | `options`, `multiple` |
| `fields` of an `object` or a `list` | the keys inside the table, as `"<key>.<inner key>"` |
| `label`, `hint`, `default`, `required` | `label`, `help`, `default`, `required` |
| `value_type`, `min`, `max`, `step` | `min`, `max`, `step` (whole numbers or not, from the values) |
| `collapsed`, `summary: "{{fields.name}}"` | `collapsed`, `summary = "{name}"` |
| `i18n: duplicate` | `shared = true` (worked out where the languages agree) |
| `i18n` structure, `media_folder`, `publish_mode` | the site's languages, `media`, `workflow` |
