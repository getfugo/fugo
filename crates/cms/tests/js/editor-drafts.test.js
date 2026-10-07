// The editor's drafts (assets/admin/cms.js), in happy-dom against a fake API (ui.js): the list
// with each draft's pull request, a draft Decap CMS left (`cms/<collection>/<slug>`), a draft
// whose files the site changed since, and saving a page that has a draft.

import { before, test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, go, sent, skip, startEditor, STYLE, text, type, until } from "./ui.js";

const SITE = {
  title: "Snacks",
  site_url: "https://example.org/",
  workflow: "review",
  languages: [{ key: "en", name: "English" }],
  default_language: "en",
  taxonomies: [],
  fields: {},
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: ["jpg"],
  max_upload: 1048576,
  sections: [{ key: "posts", title: "Posts", count: 1, style: STYLE, keys: [{ key: "title", kind: "string" }] }],
  entries: [
    { key: "posts/crisps", section: "posts", kind: "page", bundle: false, title: "Crisps", files: [{ lang: "en", path: "content/posts/crisps.md" }], resources: [] },
  ],
};
const FILES = { "content/posts/crisps.md": "---\ntitle: Crisps\n---\nThey crunch.\n" };

const PULL = { number: 27, url: "https://github.com/owner/site/pull/27", title: "Create cake “tokyo-hiyoko/index”" };
const DRAFTS = [
  { id: "posts-crisps-1a2b3c4d", entry: "posts/crisps", title: "Crisps", author: { name: "ann", email: "ann@example.org" }, pr: null },
  { id: "cake/tokyo-hiyoko/index", entry: "", title: PULL.title, author: { name: "Bo", email: "bo@example.org" }, pr: PULL },
];
const DECAP = {
  id: "cake/tokyo-hiyoko/index",
  entry: "",
  title: PULL.title,
  pr: PULL,
  files: [{ path: "content/cake/tokyo-hiyoko/index.en.md", status: "added", patch: "@@ @@\n+hiyoko" }],
  commits: [{ author: { name: "Bo", email: "bo@example.org" }, subject: PULL.title }],
  conflicts: [],
};
const STALE = {
  ...DRAFTS[0],
  files: [{ path: "content/posts/crisps.en.md", status: "modified", patch: "@@ @@\n-a\n+b" }],
  commits: [{ author: DRAFTS[0].author, subject: "Edit posts/crisps: Crisps" }],
  conflicts: ["content/posts/crisps.en.md"],
};
const DETAILS = { [DECAP.id]: DECAP, [STALE.id]: STALE };

before(() => skip || startEditor(SITE, FILES, { drafts: () => ({ drafts: DRAFTS }), draft: (q) => DETAILS[q.get("id")] ?? null }));

test("the drafts list links each draft, and its pull request when it has one", { skip }, async () => {
  await go("#/drafts", () => text($("h1")) === "Drafts" && $("table.entries"));
  const rows = $$("table.entries tr");
  assert.deepEqual(rows.map((r) => text(r.querySelector("td"))), ["Crisps", PULL.title]);
  assert.equal(rows[0].querySelectorAll("a").length, 1, "no pull request");
  const [draft, pull] = rows[1].querySelectorAll("a");
  assert.equal(draft.getAttribute("href"), "#/d/cake/tokyo-hiyoko/index");
  assert.equal(pull.getAttribute("href"), PULL.url);
  assert.equal(text(pull), "#27");
});

test("a draft of Decap CMS opens at its id, with its pull request", { skip }, async () => {
  await go("#/d/cake/tokyo-hiyoko/index", () => $(".crumbs"));
  assert.equal(text($(".crumbs")), "Drafts / cake/tokyo-hiyoko/index");
  assert.equal(text($("h1")), PULL.title);
  assert.equal($(`a[href="${PULL.url}"]`)?.getAttribute("target"), "_blank");
  assert.deepEqual($$(".change code").map(text), ["content/cake/tokyo-hiyoko/index.en.md"]);
  assert.ok(!$(`.actions a[href^="#/e/"]`), "no page to open: Decap CMS's drafts name none");
});

test("a draft whose files the site changed says publishing merges them, and may be published", { skip }, async () => {
  await go(`#/d/${STALE.id}`, () => $(".warn"));
  assert.match(text($(".warn")), /^The site changed content\/posts\/crisps\.en\.md since this draft was made: publishing brings those changes into the draft first\./);
  const publish = $$(".actions button").find((b) => text(b) === "Publish");
  assert.ok(publish && !publish.disabled, "Publish is not disabled");
});

test("a page with a draft saves to that draft", { skip }, async () => {
  await go("#/e/posts/crisps", () => $("#f-title"));
  assert.equal($(".badge.pending")?.getAttribute("href"), `#/d/${DRAFTS[0].id}`);
  type($("#f-title"), "Crispier");
  sent.length = 0;
  $(".actions .primary").click();
  await until(() => sent.length);
  assert.equal(sent[0].name, "save");
  assert.equal(sent[0].body.draft, DRAFTS[0].id);
});
