// The terms of taxonomies in the editor (assets/admin/cms.js), in happy-dom against a fake API
// (ui.js): a term with a page of its own (content/brands/lays/_index.md) and nothing below it is
// listed as that page, not as an empty folder; folders count the terms below them.

import { before, test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, go, skip, startEditor, STYLE, text, type } from "./ui.js";

/** An entry of the index: a page, or a folder's own page (`<dir>/_index`). */
function entry(key, title) {
  const kind = key.endsWith("_index") ? "section" : "page";
  return { key, section: key.split("/")[0], kind, bundle: false, title, files: [{ lang: "en", path: `content/${key}.md` }], resources: [] };
}

const SITE = {
  title: "Snacks",
  site_url: "https://example.org/",
  workflow: "review",
  languages: [{ key: "en", name: "English" }],
  default_language: "en",
  taxonomies: [
    { plural: "brands", singular: "brand", hierarchical: false, terms: ["lays", "pringles"] },
    { plural: "flavors", singular: "flavor", hierarchical: true, terms: ["cheese", "cheese/cheddar", "chili"] },
  ],
  fields: {},
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: ["jpg"],
  max_upload: 1048576,
  sections: [
    { key: "snacks", title: "Snacks", count: 2, folders: 2, style: STYLE, keys: [{ key: "title", kind: "string" }] },
    { key: "brands", title: "Brands", count: 0, folders: 2, style: STYLE, keys: [{ key: "title", kind: "string" }] },
    { key: "flavors", title: "Flavors", count: 0, folders: 3, style: STYLE, keys: [{ key: "title", kind: "string" }] },
  ],
  entries: [
    entry("snacks/_index", "Snacks"),
    entry("snacks/chips/_index", "Chips"),
    entry("snacks/chips/lays-classic", "Lays Classic"),
    entry("snacks/chips/pringles-original", "Pringles Original"),
    // A kind of snack with no snacks yet: still a folder, to make snacks in.
    entry("snacks/candy/_index", "Candy"),
    entry("brands/_index", "Brands"),
    entry("brands/lays/_index", "Lay's"),
    entry("brands/pringles/_index", "Pringles"),
    // A hierarchical taxonomy: a term with terms below it stays a folder.
    entry("flavors/_index", "Flavors"),
    entry("flavors/cheese/_index", "Cheese"),
    entry("flavors/cheese/cheddar/_index", "Cheddar"),
    entry("flavors/chili/_index", "Chili"),
  ],
};
const FILES = {
  "content/brands/lays/_index.md": "---\ntitle: Lay's\n---\nPotato chips.\n",
  "content/snacks/chips/_index.md": "---\ntitle: Chips\n---\n",
};

before(() => skip || startEditor(SITE, FILES));

/** The rows of the folder shown: title, link and what it says beside the title. */
const rows = () =>
  $$("table.entries tbody tr").map((tr) => {
    const a = tr.querySelector("td:first-child > a");
    const muted = tr.querySelector("td:first-child > .muted");
    return [text(a), a.getAttribute("href"), muted ? text(muted) : ""];
  });

test("the root: a taxonomy's section counts its terms, other folders their pages", { skip }, async () => {
  await go("#/s/", () => text($("h1") ?? { textContent: "" }) === "Pages" && $("table.entries"));
  assert.deepEqual(rows(), [
    ["Brands", "#/s/brands", "2 brands"],
    ["Flavors", "#/s/flavors", "3 flavors"],
    ["Snacks", "#/s/snacks", "2 pages"],
  ]);
});

test("the terms of a taxonomy's section are listed as their pages", { skip }, async () => {
  await go("#/s/brands", () => text($("h1")) === "Brands" && $("table.entries"));
  assert.equal($$("tr.folder").length, 0);
  assert.deepEqual(rows(), [
    ["Lay's", "#/e/brands%2Flays%2F_index", ""],
    ["Pringles", "#/e/brands%2Fpringles%2F_index", ""],
  ]);
  assert.equal($$("main > p.muted").length, 0, "not “Nothing in this folder yet.”");
});

test("in a hierarchical taxonomy, a term with terms below it stays a folder", { skip }, async () => {
  await go("#/s/flavors", () => text($("h1")) === "Flavors" && $("table.entries"));
  assert.deepEqual(rows(), [
    ["Cheese", "#/s/flavors/cheese", "1 flavor"],
    ["Chili", "#/e/flavors%2Fchili%2F_index", ""],
  ]);
  await go("#/s/flavors/cheese", () => text($("h1")) === "Cheese" && $("table.entries"));
  assert.deepEqual(rows(), [["Cheddar", "#/e/flavors%2Fcheese%2Fcheddar%2F_index", ""]]);
});

test("the filter finds terms as pages, and says where they are", { skip }, async () => {
  await go("#/s/", () => text($("h1")) === "Pages" && $("table.entries"));
  type($("input.filter"), "ched");
  assert.deepEqual(rows(), [["Cheddar", "#/e/flavors%2Fcheese%2Fcheddar%2F_index", "in flavors/cheese"]]);
  type($("input.filter"), "index");
  assert.deepEqual(rows(), [], "not by the name of their file");
  type($("input.filter"), "");
});

test("a folder that is not a term stays one, even with nothing in it", { skip }, async () => {
  await go("#/s/snacks", () => text($("h1")) === "Snacks" && $("table.entries"));
  assert.deepEqual(rows(), [
    ["Candy", "#/s/snacks/candy", "0 pages"],
    ["Chips", "#/s/snacks/chips", "2 pages"],
  ]);
});

test("a term's page: its crumbs end at the folder that lists it", { skip }, async () => {
  await go("#/e/brands%2Flays%2F_index", () => $(".doc-head code")?.textContent.includes("brands/lays"));
  assert.equal(text($(".crumbs")), "Pages / Brands");
  assert.equal($("#f-title").value, "Lay's");
  await go("#/e/snacks%2Fchips%2F_index", () => $(".doc-head code")?.textContent.includes("snacks/chips"));
  assert.equal(text($(".crumbs")), "Pages / Snacks / Chips", "a folder's own page, at its folder");
});
