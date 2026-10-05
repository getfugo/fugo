// The editor's views (assets/admin/cms.js), drawn in happy-dom against a fake API: what they
// show, and what typing and clicking in them changes. happy-dom comes from the modules of
// tools/dev/node.sh, whose directory tests/it/js.rs passes as CMS_NODE_MODULES.

import { before, test } from "node:test";
import assert from "node:assert/strict";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const modules = process.env.CMS_NODE_MODULES;
const skip = modules ? false : "CMS_NODE_MODULES is not set (tools/dev/node.sh installs happy-dom)";

const STYLE = { bundle: false, lang_suffix: false, format: "yaml", ext: "md" };
const SITE = {
  title: "Snacks",
  site_url: "https://example.org/",
  workflow: "review",
  languages: [{ key: "en", name: "English" }],
  default_language: "en",
  taxonomies: [{ plural: "tags", singular: "tag", hierarchical: false, terms: ["crisp", "sweet"] }],
  fields: { summary: { label: "Summary", help: "One line for lists" } },
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: ["jpg", "png"],
  max_upload: 1048576,
  sections: [
    { key: "posts", title: "Posts", count: 3, style: STYLE, keys: [{ key: "title", kind: "string" }, { key: "weight", kind: "number" }] },
  ],
  entries: [
    { key: "posts/_index", section: "posts", kind: "section", bundle: false, title: "Posts", files: [{ lang: "en", path: "content/posts/_index.md" }], resources: [] },
    { key: "posts/sweet/_index", section: "posts", kind: "section", bundle: false, title: "Sweet", files: [{ lang: "en", path: "content/posts/sweet/_index.md" }], resources: [] },
    { key: "posts/crisps", section: "posts", kind: "page", bundle: false, title: "Crisps", files: [{ lang: "en", path: "content/posts/crisps.md" }], resources: [] },
    { key: "posts/wafers", section: "posts", kind: "page", bundle: false, title: "Wafers", files: [{ lang: "en", path: "content/posts/wafers.md" }], resources: [] },
  ],
};
const ME = {
  email: "ann@example.org",
  roles: ["owner"],
  edit: ["**"],
  publish: true,
  workflow: "review",
  areas: [{ kind: "content", glob: "content/**", ext: ["md"] }],
  deny: [],
};
const FILES = {
  "content/posts/crisps.md": "---\ntitle: Crisps\nsummary: Thin and salty\ntags:\n  - crisp\nsizes:\n  - small\n---\nThey crunch.\n",
};

/** The API's answers by name, and the bodies of the requests that change something. */
const api = {
  site: () => SITE,
  me: () => ME,
  drafts: () => ({ drafts: [] }),
  file: (q) => (q.get("path") in FILES ? { content: Buffer.from(FILES[q.get("path")]).toString("base64"), sha: "s1" } : null),
  save: () => ({ draft: "ann-1" }),
};
const sent = [];

async function fakeFetch(url, init = {}) {
  const u = new URL(String(url), "https://example.org/admin/");
  const name = u.pathname.replace(/^\/admin\/api\//, "");
  if (init.body) sent.push({ name, body: JSON.parse(init.body) });
  const data = api[name]?.(u.searchParams);
  return new Response(JSON.stringify(data ?? { error: "not found" }), { status: data ? 200 : 404 });
}

let window, document;

before(async () => {
  if (skip) return;
  const { Window } = await import(pathToFileURL(join(modules, "happy-dom/lib/index.js")).href);
  window = new Window({ url: "https://example.org/admin/" });
  document = window.document;
  // The editor and lit-html use the browser's globals.
  for (const k of ["document", "location", "Event", "KeyboardEvent", "FormData", "DOMParser", "Node", "HTMLElement"]) {
    Object.defineProperty(globalThis, k, { value: window[k], configurable: true, writable: true });
  }
  Object.assign(globalThis, { window, fetch: fakeFetch, confirm: () => true });
  document.body.innerHTML = `<div id="app" class="loading">Loading the editor…</div>`;
  await import("../../assets/admin/cms.js");
  await until(() => document.querySelector("header"));
});

/** Waits (a second at most) until `ready` is true, and returns what it returns. */
async function until(ready) {
  for (let i = 0; i < 100; i++) {
    const v = ready();
    if (v) return v;
    await new Promise((r) => setTimeout(r, 10));
  }
  assert.fail(`not drawn: ${ready}`);
}

async function go(hash, ready) {
  window.location.hash = hash;
  return until(ready);
}

const $ = (s) => document.querySelector(s);
const $$ = (s) => [...document.querySelectorAll(s)];
const text = (el) => el.textContent.replace(/\s+/g, " ").trim();
/** The terms of the open page's terms field. */
const chips = () => $$(".terms .chip").map((c) => text(c).replace(/×$/, ""));

function type(input, value) {
  input.value = value;
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
}

test("the start page: the site, its sections and the signed-in person", { skip }, () => {
  assert.equal(text($("header .brand")), "Snacks");
  assert.equal(text($("header .user")), "ann@example.org");
  assert.ok($("header .drafts-link"), "the review workflow has drafts");
  assert.deepEqual($$(".cards .card strong").map(text), ["Posts"]);
  assert.equal($("#app").className, "", "the placeholder is gone");
  assert.ok(!$("#app").textContent.includes("Loading the editor"));
});

test("a section lists its pages, section pages first; the filter hides the others", { skip }, async () => {
  await go("#/s/posts", () => $("table.entries"));
  assert.equal($(".sidebar a.active").getAttribute("href"), "#/s/posts");
  const titles = () => $$("table.entries tbody tr:not([hidden]) td:first-child a").map(text);
  assert.deepEqual(titles(), ["Posts", "Sweet", "Crisps", "Wafers"]);
  const filter = $("input.filter");
  type(filter, "waf");
  assert.deepEqual(titles(), ["Wafers"]);
  assert.equal($("input.filter"), filter, "a redraw keeps the element, so typing goes on");
  type(filter, "");
});

test("New page asks for the folder, then opens the new page", { skip }, async () => {
  await go("#/s/posts", () => $("table.entries"));
  const newPage = $$(".title-row button").find((b) => text(b) === "New page");
  newPage.click();
  assert.deepEqual($$("form.new-page option").map(text), ["Folder…", "Posts", "Sweet"]);
  newPage.click();
  assert.equal($("form.new-page"), null, "a second click closes the form");
  newPage.click();
  $("form.new-page select").value = "posts/sweet";
  $("form.new-page input[name=title]").value = "Honey Butter";
  $("form.new-page").dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }));
  await until(() => $(".crumbs")?.textContent.includes("posts/sweet/honey-butter"));
  assert.equal($(".fields input#f-title").value, "Honey Butter");
  assert.equal(text($(".actions .primary")), "Save draft");
});

test("a page: a field per key, as the settings label them", { skip }, async () => {
  await go("#/e/posts%2Fcrisps", () => $(".crumbs")?.textContent.includes("posts/crisps") && $(".fields"));
  assert.equal($("#f-title").value, "Crisps");
  const summary = $("#f-summary").closest(".field");
  assert.equal(text(summary.querySelector("label")), "Summary×");
  assert.equal(text(summary.querySelector("small")), "One line for lists");
  assert.deepEqual(chips(), ["crisp"]);
  assert.deepEqual($$("datalist#terms-tags option").map((o) => o.value), ["crisp", "sweet"]);
  assert.equal($("textarea.body").value, "They crunch.\n");
  assert.deepEqual($$(".add-field option").map(text), ["Add a field…", "weight"]);
});

test("terms: Enter adds one and keeps the input, × removes one", { skip }, () => {
  const input = $(".terms input");
  input.value = "sweet";
  input.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  assert.deepEqual(chips(), ["crisp", "sweet"]);
  assert.equal($(".terms input"), input, "the same input, to type the next term in");
  assert.equal(input.value, "");
  $$(".terms .chip button")[0].click();
  assert.deepEqual(chips(), ["sweet"]);
});

test("a list: Add makes a row, and what is typed stays across redraws", { skip }, () => {
  const rows = () => $$(".list .row input");
  type(rows()[0], "large");
  $$(".list button.small").find((b) => text(b) === "Add").click();
  assert.deepEqual(rows().map((i) => i.value), ["large", ""]);
  $$(".list .row button")[0].click();
  assert.deepEqual(rows().map((i) => i.value), [""], "the second row is now the first");
});

test("Add a field adds the key, and the list starts over", { skip }, () => {
  const select = $(".add-field select");
  select.value = "weight";
  select.dispatchEvent(new window.Event("change", { bubbles: true }));
  assert.equal($("#f-weight").type, "number");
  assert.equal($(".add-field"), null, "no key is left to add");
});

test("Save sends the page's file as typed", { skip }, async () => {
  type($("#f-title"), "Crispy");
  type($("textarea.body"), "They crunch loudly.\n");
  sent.length = 0;
  $(".actions .primary").click();
  await until(() => sent.length);
  const { name, body } = sent[0];
  assert.equal(name, "save");
  assert.equal(body.entry, "posts/crisps");
  assert.equal(body.title, "Crispy");
  assert.deepEqual(body.changes.map((c) => c.path), ["content/posts/crisps.md"]);
  assert.equal(
    body.changes[0].content,
    "---\ntitle: Crispy\nsummary: Thin and salty\ntags:\n  - sweet\nsizes:\n  - \"\"\nweight: 0\n---\nThey crunch loudly.\n",
  );
});

test("Edit as text shows the file, and the form comes back", { skip }, async () => {
  await go("#/e/posts%2Fwafers", () => $(".crumbs")?.textContent.includes("posts/wafers") && $(".tabs"));
  await go("#/e/posts%2Fcrisps", () => $(".crumbs")?.textContent.includes("posts/crisps") && $("#f-title"));
  const toggle = $(".doc-head .toggle input");
  toggle.checked = true;
  toggle.dispatchEvent(new window.Event("change", { bubbles: true }));
  assert.ok($("textarea.raw").value.startsWith("---\ntitle: Crisps\n"));
  type($("textarea.raw"), "---\ntitle: Crunchy\n---\nText.\n");
  toggle.checked = false;
  toggle.dispatchEvent(new window.Event("change", { bubbles: true }));
  assert.equal($("#f-title").value, "Crunchy");
  assert.equal($("textarea.body").value, "Text.\n");
});

test("Preview opens beside the text, and closes", { skip }, () => {
  const button = $$(".body-editor button").find((b) => text(b) === "Preview");
  button.click();
  assert.ok($(".body-editor").classList.contains("previewing"));
  assert.equal($("iframe.preview").hidden, false);
  assert.equal(text(button), "Close preview");
  button.click();
  assert.equal($("iframe.preview").hidden, true);
  assert.ok(!$(".body-editor").classList.contains("previewing"));
});
