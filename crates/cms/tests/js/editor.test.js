// editor/folders.ts (in the built editor, assets/admin/cms.js): the folders a new page can go in.

import { test } from "node:test";
import assert from "node:assert/strict";

import { foldersOf, movedPath, shellUrl } from "../../assets/admin/cms.js";

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

test("a moved page's files: its folder, or its name, replaced", () => {
  assert.equal(movedPath("content/docs/guides/install/index.md", "docs/guides/install", "docs/install"), "content/docs/install/index.md");
  assert.equal(movedPath("content/docs/guides/install/img/a.png", "docs/guides/install", "docs/install"), "content/docs/install/img/a.png");
  assert.equal(movedPath("content/docs/guides/install.th.md", "docs/guides/install", "docs/reference/install"), "content/docs/reference/install.th.md");
  assert.equal(movedPath("content/th/docs/a/index.md", "docs/a", "docs/b/a"), "content/th/docs/b/a/index.md");
  assert.throws(() => movedPath("content/docs/other.md", "docs/a", "docs/b/a"));
});

test("the page whose layout a preview borrows: its own, else one in its folder, else its section", () => {
  const page = (key, urls, kind = "page") => ({ key, kind, section: key.split("/")[0], files: Object.entries(urls).map(([lang, url]) => ({ lang, path: `content/${key}.md`, url })) });
  const entries = [
    page("docs/guides/install/linux", { en: "/docs/guides/install/linux/", th: "/th/docs/guides/install/linux/" }),
    page("docs/guides/intro", { en: "/docs/guides/intro/" }),
    page("docs/reference/api", { en: "/docs/reference/api/", th: "/th/docs/reference/api/" }),
    page("docs/guides/_index", { en: "/docs/guides/" }, "section"),
  ];
  assert.equal(shellUrl(entries[0], "th", entries), "/th/docs/guides/install/linux/", "its own");
  const fresh = page("docs/guides/new", {});
  assert.equal(shellUrl(fresh, "en", entries), "/docs/guides/intro/", "a page of its folder");
  assert.equal(shellUrl(fresh, "th", entries), "/th/docs/guides/install/linux/", "else a page of its section");
  assert.equal(shellUrl(page("blog/x", {}), "en", entries), null, "none");
  assert.equal(shellUrl(page("docs/guides/more/_index", {}, "section"), "en", entries), "/docs/guides/", "the same kind");
});
