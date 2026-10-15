// The preview and the build of a page's draft in the editor (assets/admin/cms.js;
// editor/preview.ts, builds.ts), in happy-dom against a fake API (ui.js): the text goes where the
// site's build marked it (`data-cms-body`, alone or among the element's children), and the status
// of the draft's build shows beside its badge, with a link to the page in the build.

import { before, test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, go, skip, startEditor, STYLE, text, window } from "./ui.js";

const PREVIEW = "https://cms-1.preview.example";
const page = (name) => ({
  key: `snacks/${name}`,
  section: "snacks",
  kind: "page",
  bundle: false,
  title: name,
  files: [{ lang: "en", path: `content/snacks/${name}.md`, url: `/snacks/${name}/` }],
  resources: [],
});
const SITE = {
  title: "Snacks",
  site_url: "https://example.org/",
  workflow: "review",
  languages: [{ key: "en", name: "English" }],
  default_language: "en",
  taxonomies: [],
  fields: { title: { label: "Title", widget: "text", kind: "string", required: true } },
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: [],
  max_upload: 1048576,
  sections: [{ key: "snacks", title: "Snacks", count: 4, style: STYLE, keys: [{ key: "title", kind: "string" }] }],
  entries: ["ready", "building", "failed", "waiting", "none"].map(page),
};
const FILES = Object.fromEntries(SITE.entries.map((e) => [e.files[0].path, `---\ntitle: ${e.title}\n---\nText.\n`]));
// Each page has a draft, whose build is in the state the page is named after.
const DRAFTS = SITE.entries.map((e) => ({ id: `${e.title}-1`, entry: e.key, title: e.title, author: null, pr: null }));
const BUILDS = {
  "ready-1": { state: "ready", url: PREVIEW, log: null },
  "building-1": { state: "building", url: null, log: null },
  "failed-1": { state: "failed", url: null, log: "https://ci.example/runs/7" },
  "waiting-1": { state: "waiting", url: `${PREVIEW}/`, log: null },
  "none-1": { state: "none", url: null, log: null },
};
const asked = [];

before(
  () =>
    skip ||
    startEditor(SITE, FILES, {
      drafts: () => ({ drafts: DRAFTS }),
      build: (q) => {
        asked.push(q.get("draft"));
        return { sha: "c1", built: "c1", ...BUILDS[q.get("draft")] };
      },
    }),
);

/** Opens page `name` and waits for its draft's build to be asked about. */
async function open(name) {
  await go(`#/e/snacks%2F${name}`, () => $(".title-row h1") && asked.includes(`${name}-1`));
  // The answer is drawn after it comes.
  await new Promise((r) => setTimeout(r, 20));
}

const badges = () => $$(".title-row .build .badge").map((b) => [text(b), b.getAttribute("href")]);

test("a draft built: a link to the page in the build", { skip }, async () => {
  await open("ready");
  assert.deepEqual(badges(), [["Draft built", `${PREVIEW}/snacks/ready/`]]);
  assert.equal($(".title-row .build a").getAttribute("target"), "_blank");
});

test("a draft building, failed, or not built yet", { skip }, async () => {
  await open("building");
  assert.deepEqual(badges(), [["Building the draft…", null]]);
  await open("failed");
  assert.deepEqual(badges(), [["The draft's build failed", "https://ci.example/runs/7"]]);
  await open("waiting");
  assert.deepEqual(badges(), [
    ["Not built yet", null],
    ["Earlier build", `${PREVIEW}/snacks/waiting/`],
  ]);
  await open("none");
  assert.deepEqual(badges(), [], "nothing builds the draft: nothing shows");
});

/** A document whose text is `body` (no front matter: the preview marks no field). */
const doc = (body) => ({ parts: { body }, body, original: {}, data: {} });

test("the preview puts the text where the build marked it, among the element's children", { skip }, async () => {
  const { prepare, putText } = await import("../../assets/admin/cms.js");
  // A script among the children before the text: the build counted it.
  const published = `<html><body><article data-cms-body="2..4"><h1>Crisps</h1><script>x()</script><p>Old one.</p><p>Old two.</p><footer>F</footer></article></body></html>`;
  const html = prepare(published, "/snacks/crisps/", doc("New text."));
  assert.ok(html.includes("<!--cms-body--><p>Old one.</p><p>Old two.</p><!--/cms-body-->"), html);
  assert.ok(!html.includes("<script>"), "without scripts");

  const d = new window.DOMParser().parseFromString(html, "text/html");
  const body = d.querySelector("[data-cms-preview=body]");
  putText(body, '<p>New <img src="a.jpg"></p>', (src) => `files/${src}`, "https://example.org/snacks/crisps/");
  assert.equal(body.innerHTML, '<h1>Crisps</h1><!--cms-body--><p>New <img src="https://example.org/snacks/crisps/files/a.jpg"></p><!--/cms-body--><footer>F</footer>');
  putText(body, "<p>Newer.</p>", (src) => src, "https://example.org/");
  assert.equal(body.innerHTML, "<h1>Crisps</h1><!--cms-body--><p>Newer.</p><!--/cms-body--><footer>F</footer>", "again, as you type");
});

test("the preview puts the text into an element the build marked whole", { skip }, async () => {
  const { prepare, putText } = await import("../../assets/admin/cms.js");
  const published = `<html><body><h1>Crisps</h1><div class="text" data-cms-body><h2>Old <a href="#old">#</a></h2></div></body></html>`;
  const html = prepare(published, "/snacks/crisps/", doc("## New"));
  const d = new window.DOMParser().parseFromString(html, "text/html");
  const body = d.querySelector("[data-cms-preview=body]");
  assert.equal(body.getAttribute("class"), "text");
  putText(body, "<h2>New</h2>", (src) => src, "https://example.org/");
  assert.equal(body.innerHTML, "<h2>New</h2>");
});

test("a mark that does not fit the page: the text is looked for by what it says", { skip }, async () => {
  const { prepare } = await import("../../assets/admin/cms.js");
  const text = "A text long enough to be looked for in the page.";
  const published = `<html><body><main data-cms-body="3..9"><section><p>${text}</p></section></main></body></html>`;
  const html = prepare(published, "/snacks/crisps/", doc(text));
  assert.ok(html.includes('<p data-cms-preview="body">'), html);
});
