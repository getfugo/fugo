// The editor with the API of `fugo server` (`login: "local"`): nobody signs in, so there is no
// signing out, and the start page says whose changes go to the git repository.

import { test } from "node:test";
import assert from "node:assert/strict";

import { $$, document, skip, startEditor, STYLE, text } from "./ui.js";

const SITE = {
  title: "Snacks",
  site_url: "http://localhost:1313/",
  languages: [{ key: "en", name: "English" }],
  default_language: "en",
  taxonomies: [],
  fields: {},
  content_dir: "content",
  media: null,
  media_ref: null,
  upload_types: [],
  max_upload: 1048576,
  sections: [{ key: "posts", title: "Posts", count: 0, folders: 0, style: STYLE, keys: [] }],
  entries: [],
};
const ME = { email: "ann@example.org", roles: ["owner"], edit: ["**"], publish: true, workflow: "review", login: "local", areas: [], deny: [] };

test("on this computer, nobody signs in or out", { skip }, async () => {
  await startEditor(SITE, {}, { me: () => ME });
  assert.equal($$("header button").filter((b) => text(b) === "Sign out").length, 0);
  assert.deepEqual($$("header .user").map(text), ["ann@example.org"]);
  const page = text(document.body);
  assert.ok(page.includes("Editing the git repository on this computer as ann@example.org (owner)."), page);
});
