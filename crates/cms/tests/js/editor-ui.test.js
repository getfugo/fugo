// The editor's views (assets/admin/cms.js), drawn in happy-dom against a fake API (ui.js): what
// they show, and what typing and clicking in them changes. The fields of tables, languages and
// new pages are in editor-fields.test.js.

import { before, test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, go, sent, skip, startEditor, STYLE, text, type, until, window } from "./ui.js";

const SITE = {
  title: "Snacks",
  site_url: "https://example.org/",
  workflow: "review",
  languages: [{ key: "en", name: "English" }],
  default_language: "en",
  taxonomies: [{ plural: "tags", singular: "tag", hierarchical: false, terms: ["crisp", "sweet"] }],
  // As the build gives them: a key with values pages share, a select of several options, a key
  // only the settings name.
  fields: {
    summary: { label: "Summary", help: "One line for lists", kind: "string" },
    brand: { label: "Brand", widget: "text", kind: "string", suggestions: ["Lee", "Tom"] },
    flavours: { label: "Flavours", widget: "select", options: ["sweet", "salty"], multiple: true, kind: "list" },
    rating: { label: "Rating", widget: "number", kind: "number", unused: true },
  },
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: ["jpg", "png"],
  max_upload: 1048576,
  sections: [
    { key: "posts", title: "Posts", count: 2, folders: 1, style: STYLE, keys: [{ key: "title", kind: "string" }, { key: "weight", kind: "number" }] },
    { key: "tags", title: "Tags", count: 0, folders: 2, style: STYLE, keys: [{ key: "title", kind: "string" }] },
  ],
  entries: [
    { key: "posts/_index", section: "posts", kind: "section", bundle: false, title: "Posts", files: [{ lang: "en", path: "content/posts/_index.md" }], resources: [] },
    { key: "posts/sweet/_index", section: "posts", kind: "section", bundle: false, title: "Sweet", files: [{ lang: "en", path: "content/posts/sweet/_index.md" }], resources: [] },
    { key: "posts/crisps", section: "posts", kind: "page", bundle: false, title: "Crisps", files: [{ lang: "en", path: "content/posts/crisps.md" }], resources: [] },
    { key: "posts/wafers", section: "posts", kind: "page", bundle: false, title: "Wafers", files: [{ lang: "en", path: "content/posts/wafers.md" }], resources: [] },
    // The terms' pages of a taxonomy, in a section without an index page.
    { key: "tags/crisp/_index", section: "tags", kind: "section", bundle: false, title: "Crisp", files: [{ lang: "en", path: "content/tags/crisp/_index.md" }], resources: [] },
    { key: "tags/sweet/_index", section: "tags", kind: "section", bundle: false, title: "Sweet", files: [{ lang: "en", path: "content/tags/sweet/_index.md" }], resources: [] },
  ],
};
const FILES = {
  "content/posts/crisps.md":
    "---\ntitle: Crisps\nsummary: Thin and salty\nbrand: Tom\nflavours:\n  - sweet\ntags:\n  - crisp\nsizes:\n  - small\n---\nThey crunch.\n",
};

before(() => skip || startEditor(SITE, FILES));

/** The terms of the open page's terms field. */
const chips = () => $$(".terms .chip").map((c) => text(c).replace(/×$/, ""));

test("the start page: the site, its sections and the signed-in person", { skip }, () => {
  assert.equal(text($("header .brand")), "Snacks");
  assert.equal(text($("header .user")), "ann@example.org");
  assert.ok($("header .drafts-link"), "the review workflow has drafts");
  assert.deepEqual($$(".cards .card strong").map(text), ["Posts", "Tags"]);
  // A taxonomy's section holds the pages of its terms: folders, named after the taxonomy.
  assert.deepEqual($$(".cards .card .muted").map(text), ["2 pages · 1 folder", "2 tags"]);
  assert.deepEqual($$(".sidebar .count").map(text), ["2", "2"], "the pages, else the folders");
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
  assert.equal(text(summary.querySelector("label > span")), "Summary");
  assert.equal(text(summary.querySelector("small")), "One line for lists");
  assert.deepEqual(chips(), ["crisp"]);
  assert.deepEqual($$("datalist#terms-tags option").map((o) => o.value), ["crisp", "sweet"]);
  assert.equal($("textarea.body").value, "They crunch.\n");
  assert.deepEqual($$(".add-field option").map(text), ["Add a field…", "weight", "Rating"], "and the keys only the settings name");
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

test("the values pages share are suggested as you type", { skip }, () => {
  assert.equal($("#f-brand").getAttribute("list"), "f-brand-values");
  assert.deepEqual($$("datalist#f-brand-values option").map((o) => o.value), ["Lee", "Tom"]);
});

test("a select of several options: a checkbox each, the value a list", { skip }, () => {
  const boxes = () => $$(".choices input[type=checkbox]");
  assert.deepEqual(boxes().map((b) => b.checked), [true, false]);
  assert.deepEqual($$(".choices label").map(text), ["sweet", "salty"]);
  for (const box of [boxes()[1], boxes()[0]]) {
    box.checked = !box.checked;
    box.dispatchEvent(new window.Event("change", { bubbles: true }));
  }
  assert.deepEqual(boxes().map((b) => b.checked), [false, true]);
});

test("Add a field adds the key, and the list starts over", { skip }, () => {
  const select = $(".add-field select");
  select.value = "weight";
  select.dispatchEvent(new window.Event("change", { bubbles: true }));
  assert.equal($("#f-weight").type, "number");
  assert.equal(select.value, "");
  assert.deepEqual($$(".add-field option").map(text), ["Add a field…", "Rating"]);
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
    "---\ntitle: Crispy\nsummary: Thin and salty\nbrand: Tom\nflavours:\n  - salty\ntags:\n  - sweet\nsizes:\n  - \"\"\nweight: 0\n---\nThey crunch loudly.\n",
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
