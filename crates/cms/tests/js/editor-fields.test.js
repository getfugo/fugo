// The fields of the editor's form (assets/admin/cms.js) that the build works out across pages, in
// happy-dom against a fake API (ui.js): the keys inside tables, closed tables, values one for
// every language, defaults of new pages, and the checks before saving.

import { before, test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, choose, go, sent, skip, startEditor, STYLE, text, type, until, window } from "./ui.js";

const SITE = {
  title: "Snacks",
  site_url: "https://example.org/",
  workflow: "review",
  languages: [
    { key: "en", name: "English" },
    { key: "th", name: "Thai" },
  ],
  default_language: "en",
  taxonomies: [],
  // As the build gives them, the settings over them.
  fields: {
    title: { label: "Title", widget: "text", kind: "string" },
    type: { label: "Type", widget: "hidden", kind: "string" },
    author: { label: "Author", widget: "text", kind: "string", default: "Kitchen" },
    image: { label: "Image", widget: "image", kind: "string", shared: true },
    rating: { label: "Rating", widget: "number", kind: "number", shared: true, min: 0, max: 5, required: true },
    nutrition: { label: "Nutrition", kind: "map", collapsed: true, summary: "{calories} kcal" },
    "nutrition.calories": { label: "Calories", widget: "number", kind: "number" },
    "nutrition.fat": { label: "Fat", kind: "map" },
    "nutrition.fat.total": { label: "Total fat (g)", widget: "number", kind: "number", min: 0 },
    "nutrition.fat.saturated": { label: "Saturated", widget: "number", kind: "number" },
    parts: { label: "Parts", kind: "objects", collapsed: true, summary: "{name}: {share}%" },
    "parts.name": { label: "Name", widget: "text", kind: "string" },
    "parts.share": { label: "Share", widget: "number", kind: "number" },
  },
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: ["jpg"],
  max_upload: 1048576,
  sections: [
    {
      key: "snacks",
      title: "Snacks",
      count: 1,
      style: { ...STYLE, lang_suffix: true },
      keys: [
        { key: "title", kind: "string" },
        { key: "type", kind: "string", default: "snack" },
        { key: "rating", kind: "number" },
        { key: "author", kind: "string" },
      ],
    },
  ],
  entries: [
    {
      key: "snacks/a",
      section: "snacks",
      kind: "page",
      bundle: false,
      title: "A",
      files: [
        { lang: "en", path: "content/snacks/a.en.md" },
        { lang: "th", path: "content/snacks/a.th.md" },
      ],
      resources: [],
    },
  ],
};
const FILES = {
  "content/snacks/a.en.md":
    "---\ntitle: A\ntype: snack\nrating: 4\nimage: a.jpg\nnutrition:\n  calories: 170\n  fat:\n    total: 9\nparts:\n  - name: Rice\n    share: 60\n---\nText.\n",
  "content/snacks/a.th.md": "---\ntitle: เอ\ntype: snack\nrating: 4\nimage: a.jpg\n---\nข้อความ\n",
};

before(() => skip || startEditor(SITE, FILES));

const openA = () => go("#/e/snacks%2Fa", () => $(".doc-head code")?.textContent.includes("snacks/a") && $("#f-rating"));
const tab = (name) => $$(".tabs .tab").find((b) => text(b) === name);

test("a closed table shows a summary of its values, and its fields when opened", { skip }, async () => {
  await openA();
  const table = $("#f-nutrition-calories").closest("details");
  assert.equal(text(table.querySelector(":scope > summary")), "170 kcal");
  assert.equal(table.open, false);
  assert.equal($("#f-nutrition-calories").value, "170");
  const item = $("details.item > summary");
  assert.ok(text(item).startsWith("Parts 1: Rice: 60%"), text(item));
});

test("a table offers the keys other pages have in it, as their settings label them", { skip }, () => {
  assert.equal(text($("#f-nutrition-fat-total").closest(".field").querySelector("label > span")), "Total fat (g)");
  assert.equal($("#f-nutrition-fat-total").getAttribute("min"), "0");
  assert.equal($("#f-nutrition-fat-total").getAttribute("step"), "any", "a whole number now may not stay so");
  const add = $("#f-nutrition-fat-total").closest("fieldset").querySelector(".add-field select");
  assert.deepEqual([...add.options].map(text), ["Add…", "Saturated"]);
  choose(add, "saturated");
  assert.equal($("#f-nutrition-fat-saturated").value, "0");
  assert.equal($("#f-nutrition-fat-total").closest("fieldset").querySelector(".add-field"), null);
});

test("a value one for every language goes to each language's file", { skip }, async () => {
  assert.ok(text($("#f-rating").closest(".field")).includes("all languages"));
  type($("#f-rating"), "5");
  tab("Thai").click();
  assert.equal($("#f-rating").value, "5", "the Thai file has it too");
  assert.equal($("#f-title").value, "เอ", "a key of each language stays its own");
  sent.length = 0;
  $(".actions .primary").click();
  await until(() => sent.length);
  const files = Object.fromEntries(sent[0].body.changes.map((c) => [c.path, c.content]));
  assert.match(files["content/snacks/a.en.md"], /\nrating: 5\n/);
  assert.match(files["content/snacks/a.th.md"], /\nrating: 5\n/);
  // The page opens again after a save, from the (fake) repository.
  await until(() => $("#f-rating")?.value === "4");
});

test("Save checks the values: required ones, and numbers within their bounds", { skip }, async () => {
  await openA();
  sent.length = 0;
  type($("#f-rating"), "9");
  $(".actions .primary").click();
  assert.match(text($("#toast")), /^Not saved: English: Rating is above 5; Thai: Rating is above 5$/);
  type($("#f-rating"), "");
  $(".actions .primary").click();
  assert.match(text($("#toast")), /Rating needs a value/);
  await new Promise((r) => setTimeout(r, 30));
  assert.equal(sent.length, 0, "nothing was sent");
});

test("a new page starts with its section's values and the settings' defaults", { skip }, async () => {
  await go("#/s/snacks", () => $("table.entries"));
  $$(".title-row button").find((b) => text(b) === "New page").click();
  await until(() => $(".doc-head code")?.textContent.includes("snacks/honey-butter"));
  assert.equal($("#f-author").value, "Kitchen");
  assert.equal($("#f-type"), null, "a hidden key is not shown…");
  const raw = $(".doc-head .toggle input");
  raw.checked = true;
  raw.dispatchEvent(new window.Event("change", { bubbles: true }));
  assert.match($("textarea.raw").value, /\ntype: snack\n/, "…but kept");
});
