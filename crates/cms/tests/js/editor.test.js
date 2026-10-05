// editor/folders.ts (in the built editor, assets/admin/cms.js): the folders a new page can go in.

import { test } from "node:test";
import assert from "node:assert/strict";

import { foldersOf } from "../../assets/admin/cms.js";

const ENTRIES = [
  { key: "docs/_index", kind: "section", title: "Documentation" },
  { key: "docs/guides/_index", kind: "section", title: "Guides" },
  { key: "docs/guides/install/_index", kind: "section", title: "Installing" },
  { key: "docs/guides/install/linux", kind: "page", title: "Linux" },
  { key: "docs/reference/_index", kind: "section", title: "Reference" },
  { key: "docs-old/_index", kind: "section", title: "Elsewhere" },
  { key: "tags/rust/_index", kind: "section", title: "Rust" },
  { key: "_index", kind: "home", title: "Home" },
];

test("the folders below a section, in key order, with their depth", () => {
  assert.deepEqual(foldersOf({ key: "docs" }, ENTRIES), [
    { key: "docs/guides", title: "Guides", depth: 1 },
    { key: "docs/guides/install", title: "Installing", depth: 2 },
    { key: "docs/reference", title: "Reference", depth: 1 },
  ]);
});

test("a section without folders, and the root section, have none", () => {
  assert.deepEqual(foldersOf({ key: "blog" }, ENTRIES), []);
  assert.deepEqual(foldersOf({ key: "" }, ENTRIES), []);
});
