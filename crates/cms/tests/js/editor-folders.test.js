// The folders of the editor (assets/admin/cms.js), in happy-dom against a fake API (ui.js) that
// keeps what is saved and the drafts: a folder's view at any depth, the filter below it, new
// folders and sections, and a new folder's page saved with the first page in it.

import { before, test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, go, sent, skip, startEditor, STYLE, text, type, until } from "./ui.js";

/** An entry of the index: a page, a bundle, or a folder's own page (`<dir>/_index`). */
function entry(key, title, { kind = key.endsWith("_index") ? "section" : "page", bundle = false } = {}) {
  const path = bundle ? `content/${key}/index.md` : `content/${key}.md`;
  return { key, section: key.includes("/") ? key.split("/")[0] : "", kind, bundle, title, files: [{ lang: "en", path }], resources: [] };
}

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
  sections: [
    { key: "", title: "Pages", count: 1, style: STYLE, keys: [{ key: "title", kind: "string" }] },
    { key: "snacks", title: "Snacks", count: 3, style: { ...STYLE, bundle: true }, keys: [{ key: "title", kind: "string" }] },
  ],
  entries: [
    entry("_index", "Snack site", { kind: "home" }),
    entry("about", "About"),
    entry("snacks/_index", "Snacks"),
    entry("snacks/chips/_index", "Chips"),
    entry("snacks/chips/potato-chips/_index", "Potato chips"),
    entry("snacks/chips/potato-chips/lays-classic", "Lays Classic", { bundle: true }),
    // A folder without a page of its own.
    entry("snacks/chips/potato-chips/wavy/ruffles", "Ruffles", { bundle: true }),
    entry("snacks/chips/tortilla", "Tortilla", { bundle: true }),
  ],
};
const FILES = {
  "content/snacks/chips/potato-chips/lays-classic/index.md": "---\ntitle: Lays Classic\n---\nThin.\n",
};

/** The drafts the fake API keeps (one per page, as the Worker's), and the files it was asked for
 * (`<path> <draft>`). */
const drafts = [];
const reads = [];

before(
  () =>
    skip ||
    startEditor(SITE, FILES, {
      drafts: () => ({ drafts }),
      file: (q) => {
        reads.push(`${q.get("path")} ${q.get("draft") ?? ""}`.trim());
        const t = FILES[q.get("path")];
        return t === undefined ? null : { content: Buffer.from(t).toString("base64"), sha: "s1" };
      },
      save: (_, body) => {
        for (const c of body.changes) if (c.content !== undefined) FILES[c.path] = c.content;
        const id = `d-${body.entry.replaceAll("/", "-")}`;
        if (!drafts.some((d) => d.id === id)) drafts.push({ id, entry: body.entry, title: body.title, author: null });
        return { draft: id };
      },
    }),
);

const titles = () => $$("table.entries tbody td:first-child > a").map(text);
const button = (name) => $$(".title-row button, .title-row a.button").find((b) => text(b) === name);
const answer = (title) => (globalThis.prompt = () => title);

test("the root: the sections as folders, then the pages at the top", { skip }, async () => {
  await go("#/s/", () => text($("h1") ?? { textContent: "" }) === "Pages");
  assert.equal($(".crumbs"), null);
  assert.equal($(".sidebar a.active").getAttribute("href"), "#/s/");
  assert.deepEqual(titles(), ["Snacks", "About"]);
  assert.equal(text($("tr.folder .muted")), "3 pages", "the pages anywhere below it");
  assert.ok(button("New page") && button("New section"));
  assert.equal(button("Edit home page").getAttribute("href"), "#/e/_index");
});

test("a folder at any depth: its folders and pages, and the way back up", { skip }, async () => {
  await go("#/s/snacks/chips", () => text($("h1")) === "Chips");
  assert.equal(text($(".crumbs")), "Pages / Snacks");
  assert.deepEqual($$(".crumbs a").map((a) => a.getAttribute("href")), ["#/s/", "#/s/snacks"]);
  assert.equal($(".sidebar a.active").getAttribute("href"), "#/s/snacks");
  assert.deepEqual(titles(), ["Potato chips", "Tortilla"]);
  assert.ok(button("New folder"), "below the root, a folder");

  await go("#/s/snacks/chips/potato-chips", () => text($("h1")) === "Potato chips");
  assert.deepEqual(titles(), ["Wavy", "Lays Classic"], "a folder without a page of its own, named after it");
  assert.equal($("tr.folder a").getAttribute("href"), "#/s/snacks/chips/potato-chips/wavy");

  await go("#/s/snacks/chips/potato-chips/wavy", () => text($("h1")) === "Wavy");
  assert.deepEqual(titles(), ["Ruffles"]);
  assert.equal(button("Edit folder page"), undefined);

  await go("#/s/snacks/nothing", () => text($("h1")) === "Not found");
});

test("the filter finds what is below the folder, and says where", { skip }, async () => {
  await go("#/s/snacks", () => text($("h1")) === "Snacks");
  type($("input.filter"), "ruff");
  assert.deepEqual(titles(), ["Ruffles"]);
  assert.equal(text($(".where")), "in chips/potato-chips/wavy");
  type($("input.filter"), "zzz");
  assert.equal(text($("main > p.muted")), "Nothing matches.");
  type($("input.filter"), "");
  assert.deepEqual(titles(), ["Chips"]);
});

test("a page: crumbs to each of its folders, and Move to any of its section's", { skip }, async () => {
  await go("#/e/snacks%2Fchips%2Fpotato-chips%2Flays-classic", () => $(".doc-head code")?.textContent.includes("lays-classic"));
  assert.equal(text($(".crumbs")), "Pages / Snacks / Chips / Potato chips");
  assert.deepEqual(
    $$("form.move option").map(text),
    ["Move to…", "Snacks", "Chips", "Potato chips", "Wavy"],
  );
});

test("New page refuses the name of a folder", { skip }, async () => {
  await go("#/s/snacks", () => text($("h1")) === "Snacks");
  answer("Chips");
  button("New page").click();
  assert.equal(text($("#toast")), "snacks/chips exists already");
  assert.equal(location.hash, "#/s/snacks");
});

test("New folder opens the new folder; its page is saved with the first page in it", { skip }, async () => {
  await go("#/s/snacks/chips", () => text($("h1")) === "Chips");
  answer("Extruded");
  button("New folder").click();
  await until(() => text($("h1")) === "Extruded");
  assert.equal(location.hash, "#/s/snacks/chips/extruded");
  assert.equal(text($(".title-row .badge")), "new");
  assert.equal(text($(".crumbs")), "Pages / Snacks / Chips");

  answer("Cheese Puffs");
  button("New page").click();
  await until(() => $(".doc-head code")?.textContent.includes("cheese-puffs"));
  assert.equal(text($(".doc-head code")), "content/snacks/chips/extruded/cheese-puffs/index.md", "a bundle, as the section's pages");
  assert.equal(text($(".crumbs")), "Pages / Snacks / Chips / Extruded");
  sent.length = 0;
  $(".actions .primary").click();
  await until(() => sent.length);
  const { body } = sent[0];
  assert.equal(body.entry, "snacks/chips/extruded/cheese-puffs");
  assert.deepEqual(body.changes.map((c) => c.path), ["content/snacks/chips/extruded/_index.md", "content/snacks/chips/extruded/cheese-puffs/index.md"]);
  assert.equal(body.changes[0].content, "---\ntitle: Extruded\n---\n");
  assert.equal(body.changes[0].base, null);

  await go("#/s/snacks/chips/extruded", () => text($("h1")) === "Extruded" && $("table.entries"));
  assert.equal($(".title-row .badge"), null, "saved");
  assert.deepEqual(titles(), ["Cheese Puffs"]);
});

test("the page of a folder saved with a page is in that page's draft", { skip }, async () => {
  button("Edit folder page").click();
  await until(() => $(".doc-head code")?.textContent.includes("extruded/_index.md"));
  assert.ok(reads.includes("content/snacks/chips/extruded/_index.md d-snacks-chips-extruded-cheese-puffs"), reads.join("\n"));
  assert.equal($("#f-title").value, "Extruded");
  type($("#f-title"), "Extruded snacks");
  sent.length = 0;
  $(".actions .primary").click();
  await until(() => sent.length);
  assert.equal(sent[0].body.entry, "snacks/chips/extruded/cheese-puffs", "the same draft");
  assert.deepEqual(sent[0].body.changes.map((c) => c.path), ["content/snacks/chips/extruded/_index.md"]);
});

test("New section: a folder at the top, its pages written like the largest section's", { skip }, async () => {
  await go("#/", () => $(".cards"));
  answer("Recipes");
  $$(".title-row button").find((b) => text(b) === "New section").click();
  await until(() => text($("h1")) === "Recipes");
  assert.equal(location.hash, "#/s/recipes");
  assert.equal(text($(".crumbs")), "Pages");
  answer("Pad Thai");
  button("New page").click();
  await until(() => $(".doc-head code")?.textContent.includes("pad-thai"));
  assert.equal(text($(".doc-head code")), "content/recipes/pad-thai/index.md");
  sent.length = 0;
  $(".actions .primary").click();
  await until(() => sent.length);
  assert.deepEqual(sent[0].body.changes.map((c) => c.path), ["content/recipes/_index.md", "content/recipes/pad-thai/index.md"]);
  await until(() => $$(".sidebar a").some((a) => a.getAttribute("href") === "#/s/recipes"));
});
