---
title: CMS editor
description: The settings of the browser editor (`[cms]`) — its path and workflow, the git repository, the Cloudflare Access sign-in, roles, field hints — and the Worker's secrets.
weight: 95
---

`[cms]` adds the browser editor described in [Editing in the browser](/content-management/cms/)
to a build. Put it in the configuration of the environment that deploys it —
`config/production/cms.toml` holds the table's keys — so that other builds have no editor. Keys
are case-insensitive, like every setting.

```toml {title="config.toml"}
[cms]
path = "admin"
workflow = "review"
media = "static/images/uploads"
maxUpload = 10

[cms.git]
host = "github"
repo = "you/site"
branch = "main"

[cms.login]
provider = "cloudflare-access"
team = "your-team"
aud = "4714c1358e5d…"

[cms.roles.writer]
edit = ["content/blog/**"]

[cms.roles.owner]
edit = ["**"]
publish = true

[cms.fields.summary]
label = "Short summary"
help = "Shown in lists and in search results."
```

## Settings

`path`
: The editor's directory under the site, `admin` by default: the editor is `/admin/`, its API
  `/admin/api/`. The editor replaces a page of the site at that path (with a warning).

`title`
: The editor's title. Default: the site's `title`.

`workflow`
: `review` (default): saves go to a draft per page, and roles with `publish` publish drafts.
  `direct`: every save is a commit to the branch.

`media`
: A directory for uploads to pages that are not bundles, inside a static directory, `assets/`
  or the content directory (`static/images/uploads`). Front matter names its files by their URL
  path for a static directory (`/images/uploads/photo.jpg`), else by their path below the
  directory. Default: none (uploads go into page bundles only).

`maxUpload`
: The largest file a save may write, in MiB, 1 to 25. Default: 10.

`html`
: Let roles write `.html` and `.htm` content files. They are raw HTML pages, whose scripts run
  on the editor's own address (see [security notes](/content-management/cms/#security-notes)).
  Default: false.

## Git

`[cms.git]` is the repository the editor commits to.

`host`
: `github`, the only one for now.

`repo`
: `owner/name` (required).

`branch`
: The branch the site is built from, which the editor publishes to. Default: `main`.

`dir`
: The project's directory in the repository, when it is not the repository's root (`site`).
  Default: found from the `.git` directory above the project; without one, the build warns and
  takes the root.

## Login

`[cms.login]` says how people sign in.

`provider`
: `cloudflare-access`, the only one for now: [Cloudflare Access](https://developers.cloudflare.com/cloudflare-one/policies/access/)
  signs people in (with Google, GitHub, Microsoft, a one-time PIN, …) before they reach the
  editor, and the Worker checks the token it adds to every request.

`team`
: The Access team: its name (`your-team`) or domain (`your-team.cloudflareaccess.com`)
  (required).

`aud`
: The audience (AUD) tag of the Access application that protects the editor, or a list of them
  (required).

## Roles

`[cms.roles.<name>]` is what one role may do; the `CMS_USERS` secret gives people roles. At least
one role is required.

`edit`
: Globs of the files the role may create, change and delete, relative to the project directory:
  `["content/blog/**", "content/**/*.th.md"]`. `*` matches any part of a name, `**` any part of
  a path, `?` one character, `{a,b}` either alternative, `[abc]` one of the characters. The
  editor writes only content, data, i18n, the `params` and `menus` files of `config/_default/`
  and the media directory, whatever the globs say; see
  [who may change what](/content-management/cms/#roles-who-may-change-what).

`publish`
: Whether the role may publish drafts (and discard anyone's). Default: false.

## Fields

The build gives the editor a field for every front matter key the content uses, and for every
key inside its tables, so most sites need no settings:

- a label from the key (`image_preview`: *Image preview*);
- a widget from the kind of its values (dates, numbers, yes/no, lists, tables) and from its name
  (an image picker for `image`, `cover`, `photo`…; a text box for `description`, `summary`…). A
  key whose pages hold values of different kinds shows each value as it is;
- the values pages share, which the editor suggests as you type (a brand, a type, an author; not
  the values that name one page, like `title` or `slug`). Taxonomy keys suggest the terms the
  site uses;
- the keys of a table across all pages: a page whose `nutrition` table lacks `fat` can add it
  under the table's *Add…*;
- whether a key's value is one for every language: when nine in ten of the pages that have the
  key in several languages, at least, give it the same value in each (an image, a rating, a
  date; the order of a table's keys and empty values do not count), an edit in one language
  goes to the page's other language files too (marked *all languages*). `title`,
  `description`, `summary`, `keywords` and `draft` stay each language's own;
- the value new pages of a section start with: the one every page of the section gives the key
  (three pages at least, half of the section's pages at least), such as a `type` or a `layout`.

`[cms.fields.<key>]` changes a field (the key matched ignoring case; a key inside a table by its
path, `"nutrition.fat.total"`, the keys of a list's tables below the list's key), setting by
setting: what you leave out stays as the build works it out. A key that no page has yet, with
settings, is offered under *Add a field* in every section (inside a table, under its *Add…*).
`fugo cms fields` prints every field as the build gives it to the editor, as these settings:
copy the ones you want to change.

```toml
[cms.fields.summary]
help = "Shown in lists and in search results."   # the label and the text box stay

[cms.fields.flavours]
label = "Flavours"
options = ["sweet", "salty", "spicy"]
multiple = true                                  # a checkbox each; the value is a list

[cms.fields.rating]
min = 0
max = 5
required = true

[cms.fields.nutrition]
collapsed = true
summary = "{calories} kcal, {fat.total} g fat"

[cms.fields."nutrition.fat.total"]
label = "Total fat (g)"

[cms.fields.author]
default = "The kitchen"

[cms.fields.sku]
widget = "hidden"
```

`label`
: The field's label. Default: the key, in words.

`help`
: A line of help under the field.

`widget`
: `text`, `textarea`, `number`, `boolean`, `date`, `select` (with `options`), `list`, `image`
  (a choice of the page's files and the uploads), or `hidden` (not shown; the value is kept).

`options`
: The choices of a `select` (a dropdown). Options without a `widget` make a select.

`multiple`
: A select of any number of its `options`, a checkbox each; the value is a list. A select of a
  key whose pages hold lists is one already. Needs `options`.

`default`
: The key's value on a new page (and when it is added to a page), over the value its section's
  pages share. With `widget = "hidden"`, a value new pages get without showing it.

`min`, `max`, `step`
: The bounds of a number and the step between numbers. The editor does not save a number out of
  bounds.

`required`
: The key needs a value: the editor does not save a page without one, on the pages of a section
  whose pages use the key (or that have it). A key inside a table needs one where the page has
  the table it is in.

`shared`
: `true`: one value for every language, an edit goes to each language's file of the page;
  `false`: each language keeps its own, where the build would share it.

`collapsed`
: A table, or each table of a list of tables, shown closed with a summary of its values.

`summary`
: The summary of a closed table: its values in place of `{key}` (`{fat.total}` for a key of a
  table inside it). Default: its first three values.

## Secrets

The Worker reads these from its secrets (`npx wrangler secret put NAME`), never from the
repository:

`CMS_USERS`
: JSON: emails, or `@domain` for an email domain, to the roles they have:
  `{"ann@gmail.com": ["writer"], "@example.org": ["writer"]}` (required).

`CMS_GITHUB_APP_ID`, `CMS_GITHUB_APP_KEY`
: A GitHub App's ID and private key (PEM), the app installed on the repository with *Contents:
  Read and write*.

`CMS_GITHUB_TOKEN`
: Instead of an app: a fine-grained token for the repository with *Contents: Read and write*.

`CMS_DEV_USER`
: For `wrangler dev` only (in `.dev.vars`): the email that requests to `localhost` sign in as,
  since there is no Cloudflare Access locally. Deployed Workers ignore it.
