// The sidebar's tree of folders (assets/admin/cms.js), in happy-dom against a fake API (ui.js):
// the sections, their folders at any depth, opened and closed in place, and the folder shown
// marked with the folders above it open. The terms that lists show as pages are not in it.

import { before, test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, go, skip, startEditor, STYLE, text, type } from "./ui.js";

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
    { plural: "brands", singular: "brand", hierarchical: false, terms: ["lays"] },
    { plural: "flavors", singular: "flavor", hierarchical: true, terms: ["cheese", "cheese/cheddar", "chili"] },
  ],
  fields: {},
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: ["jpg"],
  max_upload: 1048576,
  sections: [
    { key: "", title: "Pages", count: 1, folders: 0, style: STYLE, keys: [{ key: "title", kind: "string" }] },
    { key: "snacks", title: "Snacks", count: 2, folders: 3, style: STYLE, keys: [{ key: "title", kind: "string" }] },
    { key: "brands", title: "Brands", count: 0, folders: 1, style: STYLE, keys: [{ key: "title", kind: "string" }] },
    { key: "flavors", title: "Flavors", count: 0, folders: 3, style: STYLE, keys: [{ key: "title", kind: "string" }] },
  ],
  entries: [
    entry("about", "About"),
    entry("snacks/_index", "Snacks"),
    entry("snacks/chips/_index", "Chips"),
    entry("snacks/chips/potato-chips/_index", "Potato chips"),
    entry("snacks/chips/potato-chips/lays-classic", "Lays Classic"),
    entry("snacks/chips/tortilla", "Tortilla"),
    entry("snacks/candy/_index", "Candy"),
    entry("brands/_index", "Brands"),
    entry("brands/lays/_index", "Lay's"),
    entry("flavors/_index", "Flavors"),
    entry("flavors/cheese/_index", "Cheese"),
    entry("flavors/cheese/cheddar/_index", "Cheddar"),
    entry("flavors/chili/_index", "Chili"),
  ],
};
const FILES = {
  "content/snacks/chips/potato-chips/lays-classic.md": "---\ntitle: Lays Classic\n---\n",
  "content/brands/lays/_index.md": "---\ntitle: Lay's\n---\n",
};

before(() => skip || startEditor(SITE, FILES));

/** The rows of the tree: name, toggle (`+` closed, `-` open, ` ` none), `*` for the marked one. */
const tree = () =>
  $$(".sidebar .node").map((n) => {
    const toggle = n.querySelector("button.twisty")?.getAttribute("aria-expanded");
    return `${toggle === "true" ? "-" : toggle === "false" ? "+" : " "} ${text(n.querySelector(".name"))}${n.classList.contains("active") ? " *" : ""}`;
  });
const toggleOf = (name) => $$(".sidebar .node").find((n) => text(n.querySelector(".name")) === name).querySelector("button.twisty");

test("the start page: the sections, closed; a section of terms only has nothing to open", { skip }, async () => {
  await go("#/", () => $(".cards"));
  assert.deepEqual(tree(), ["  Pages", "+ Snacks", "  Brands", "+ Flavors"]);
  assert.deepEqual($$(".sidebar .count").map(text), ["1", "2", "1", "3"]);
});

test("a toggle opens and closes a folder in place, without leaving the view", { skip }, async () => {
  await go("#/s/", () => text($("main h1")) === "Pages");
  type($("input.filter"), "ab");
  const filter = $("input.filter");
  toggleOf("Snacks").click();
  assert.deepEqual(tree(), ["  Pages *", "- Snacks", "  Candy", "+ Chips", "  Brands", "+ Flavors"]);
  toggleOf("Chips").click();
  assert.deepEqual(tree().slice(1, 6), ["- Snacks", "  Candy", "- Chips", "  Potato chips", "  Brands"]);
  assert.equal(location.hash, "#/s/");
  assert.ok($("input.filter") === filter && filter.value === "ab", "the view is not drawn again");
  toggleOf("Snacks").click();
  assert.deepEqual(tree(), ["  Pages *", "+ Snacks", "  Brands", "+ Flavors"]);
  type($("input.filter"), "");
});

test("the folder shown is marked, and it and the folders above it open", { skip }, async () => {
  await go("#/s/snacks/chips", () => text($("main h1")) === "Chips");
  assert.deepEqual(tree(), ["  Pages", "- Snacks", "  Candy", "- Chips *", "  Potato chips", "  Brands", "+ Flavors"]);
  assert.equal($(".sidebar a[aria-current=page]").getAttribute("href"), "#/s/snacks/chips");

  // Closed by hand, a folder stays closed while the view stays; where the view goes, it opens.
  toggleOf("Snacks").click();
  assert.deepEqual(tree(), ["  Pages", "+ Snacks", "  Brands", "+ Flavors"]);
  await go("#/s/snacks/chips/potato-chips", () => text($("main h1")) === "Potato chips");
  assert.deepEqual(tree().slice(1, 5), ["- Snacks", "  Candy", "- Chips", "  Potato chips *"]);
});

test("a page marks the folder whose list has it; a term's page, its taxonomy's", { skip }, async () => {
  await go("#/e/snacks%2Fchips%2Fpotato-chips%2Flays-classic", () => $(".doc-head code")?.textContent.includes("lays-classic"));
  assert.ok(tree().includes("  Potato chips *"), tree().join("\n"));
  await go("#/e/brands%2Flays%2F_index", () => $(".doc-head code")?.textContent.includes("brands/lays"));
  assert.ok(tree().includes("  Brands *"), tree().join("\n"));
});

test("in a hierarchical taxonomy, the terms with terms below them are the tree's folders", { skip }, async () => {
  await go("#/s/flavors", () => text($("main h1")) === "Flavors");
  assert.deepEqual(tree().slice(-2), ["- Flavors *", "  Cheese"]);
  // A term's folder, reached by its address, marks the folder above it.
  await go("#/s/flavors/chili", () => text($("main h1")) === "Chili");
  assert.deepEqual(tree().slice(-2), ["- Flavors *", "  Cheese"]);
});
