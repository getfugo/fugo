// The images of a page in the editor (assets/admin/cms.js; editor/images.ts, files.ts) and the
// keys a translation lacks (editor/form.ts), in happy-dom against a fake API (ui.js): image fields
// with their pictures and uploads, the pictures of a bundle's files, images where the site
// publishes them, and the title a translation has yet to give.

import { before, test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, button, go, press, sent, skip, startEditor, STYLE, text, type, until, window } from "./ui.js";

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
  fields: {
    title: { label: "Title", widget: "text", kind: "string", required: true },
    description: { label: "Description", widget: "textarea", kind: "string" },
    author: { label: "Author", widget: "text", kind: "string", shared: true },
    date: { label: "Date", widget: "date", kind: "date", required: true, shared: true },
    image: { label: "Image", widget: "image", kind: "string", shared: true },
  },
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: ["jpg", "pdf"],
  max_upload: 1048576,
  sections: [
    {
      key: "snacks",
      title: "Snacks",
      count: 1,
      style: { ...STYLE, bundle: true, lang_suffix: true },
      keys: ["title", "description", "author", "date", "image"].map((key) => ({ key, kind: "string" })),
    },
  ],
  entries: [
    {
      key: "snacks/crisps",
      section: "snacks",
      kind: "page",
      bundle: true,
      title: "Crisps",
      files: [
        { lang: "en", path: "content/snacks/crisps/index.en.md", url: "/snacks/crisps/" },
        { lang: "th", path: "content/snacks/crisps/index.th.md", url: "/th/snacks/crisps/" },
      ],
      resources: ["content/snacks/crisps/bag.jpg", "content/snacks/crisps/notes.pdf", "content/snacks/crisps/open.jpg"],
    },
  ],
};
const FILES = {
  "content/snacks/crisps/index.en.md": "---\ntitle: Crisps\ndescription: Salted potato crisps\nauthor: Ann\ndate: 2020-01-02T00:00:00Z\nimage: bag.jpg\n---\n![Open](open.jpg)\n",
  "content/snacks/crisps/index.th.md": "---\nimage: bag.jpg\n---\n![เปิด](open.jpg)\n",
};

// Someone who may write the pages and the images of the content.
const ME = {
  email: "ann@example.org",
  roles: ["owner"],
  edit: ["**"],
  publish: true,
  workflow: "review",
  areas: [{ kind: "content", glob: "content/**", ext: ["md", "jpg", "pdf"] }],
  deny: [],
};

before(() => skip || startEditor(SITE, FILES, { me: () => ME }));

const open = () => go("#/e/snacks%2Fcrisps", () => $(".doc-head code")?.textContent.includes("index.en.md") && $("#f-image"));
const tab = (name) => $$(".tabs .tab").find((b) => text(b) === name);
const imageField = () => $("#f-image").closest(".image-input");

test("an image field shows its file's picture, and the page's images to choose, each with its picture", { skip }, async () => {
  await open();
  const thumb = imageField().querySelector(".thumb");
  assert.equal(thumb.getAttribute("href"), "https://example.org/snacks/crisps/bag.jpg", "it opens the image");
  assert.equal(thumb.querySelector("img").getAttribute("src"), "https://example.org/snacks/crisps/bag.jpg");
  const options = $$("#f-image md-select-option");
  assert.deepEqual(
    options.map((o) => o.value),
    ["", "bag.jpg", "open.jpg"],
    "the bundle's images, not its other files",
  );
  assert.equal(options[2].querySelector("img[slot=start]").getAttribute("src"), "https://example.org/snacks/crisps/open.jpg");
  assert.equal(imageField().querySelector("input[type=file]").getAttribute("accept"), ".jpg", "images only");
});

test("the files of a page show their pictures, and their whole names", { skip }, () => {
  const items = $$(".file-list li");
  assert.deepEqual(
    items.map((li) => text(li.querySelector("code"))),
    ["bag.jpg", "notes.pdf", "open.jpg"],
  );
  assert.equal(items[0].querySelector(".thumb img").getAttribute("src"), "https://example.org/snacks/crisps/bag.jpg");
  assert.equal(items[1].querySelectorAll("img").length, 0, "a file that is not an image has no picture");
  assert.equal(text(items[1].querySelector(".thumb")), "pdf");
});

test("an image uploaded in its field is chosen, and shows from the browser's copy until it is saved", { skip }, async () => {
  const input = imageField().querySelector("input[type=file]");
  Object.defineProperty(input, "files", { value: [new File(["jpeg bytes"], "fresh.jpg", { type: "image/jpeg" })], configurable: true });
  input.dispatchEvent(new window.Event("change", { bubbles: true }));
  await until(() => $("#f-image")?.value === "fresh.jpg");
  assert.match(imageField().querySelector(".thumb img").getAttribute("src"), /^data:image\/jpeg;base64,/);
  const added = $$(".file-list li").find((li) => text(li).startsWith("fresh.jpg"));
  assert.ok(added?.textContent.includes("not saved"));
  const raw = $(".doc-head .toggle md-switch");
  raw.selected = true;
  raw.dispatchEvent(new window.Event("change", { bubbles: true }));
  assert.match($("textarea.raw").value, /\nimage: fresh\.jpg\n/);
  // Opened again: as the (fake) repository has it.
  await go("#/s/snacks", () => $("table.entries"));
});

test("a translation shows the page's images from where the site publishes them: with the default language's page", { skip }, async () => {
  await open();
  tab("Thai").click();
  await until(() => $(".doc-head code")?.textContent.includes("index.th.md"));
  assert.equal(imageField().querySelector(".thumb img").getAttribute("src"), "https://example.org/snacks/crisps/bag.jpg");
  const img = await until(() => $(".rich-box img"));
  assert.equal(img.getAttribute("src"), "https://example.org/snacks/crisps/open.jpg", "not below /th/");
});

test("a translation shows the keys it lacks: required ones, and text another language has, to copy", { skip }, async () => {
  const labels = $$(".fields > .field label").map(text);
  assert.deepEqual(labels, ["Title*", "Description", "Date* all languages", "Image all languages"], "in the other language's order");
  assert.equal($("#f-title").value, "");
  const copyRow = (id) => $(id).closest(".field").querySelector(".copy-from");
  assert.equal(text(copyRow("#f-title")), "English: Crisps Copy");
  assert.ok(!$$(".add-field md-menu-item").map(text).includes("Title"), "not offered to add as well");
  assert.equal($$(".fields > .field").filter((f) => f.querySelector("md-icon-button.remove")).length, 1, "nothing to remove but the image");

  await press(button("Copy", copyRow("#f-title")));
  assert.equal($("#f-title").value, "Crisps");
  type($("#f-description"), "มันฝรั่งทอด");
  await press(button("Copy", copyRow("#f-date")));
  sent.length = 0;
  await press($(".actions .primary"));
  await until(() => sent.length);
  const changes = Object.fromEntries(sent[0].body.changes.map((c) => [c.path, c.content]));
  assert.equal(
    changes["content/snacks/crisps/index.th.md"],
    "---\ntitle: Crisps\ndescription: มันฝรั่งทอด\ndate: 2020-01-02T00:00:00Z\nimage: bag.jpg\n---\n![เปิด](open.jpg)\n",
    "each key where the form shows it",
  );
  assert.ok(!("content/snacks/crisps/index.en.md" in changes), "the English file has that date already");
});
