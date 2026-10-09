// The editor when no one is signed in: a link for each way to sign in the API lists, which comes
// back to the route shown; then, signed in, signing out.

import { test } from "node:test";
import assert from "node:assert/strict";

import { $, $$, document, sent, skip, startEditor, STYLE, text, until, window } from "./ui.js";

const SITE = {
  title: "Snacks",
  site_url: "https://example.org/",
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
const ME = { email: "ann@example.org", roles: ["owner"], edit: ["**"], publish: true, workflow: "direct", login: "oauth", areas: [], deny: [] };
const SIGN_IN = [
  { provider: "github", label: "GitHub" },
  { provider: "google", label: "Google" },
];

test("signing in, and out", { skip }, async () => {
  let signedIn = false;
  const notSignedIn = () => new Response(JSON.stringify({ error: "not signed in", signIn: SIGN_IN }), { status: 401 });
  await startEditor(
    SITE,
    {},
    { site: () => (signedIn ? SITE : notSignedIn()), me: () => (signedIn ? ME : notSignedIn()), logout: () => ({ signedOut: true }) },
    { url: "https://example.org/admin/#/s/posts", ready: () => $(".sign-in") },
  );
  const links = $$(".sign-in a");
  assert.deepEqual(links.map(text), ["Sign in with GitHub", "Sign in with Google"]);
  assert.equal(links[0].getAttribute("href"), "https://example.org/admin/api/login/github?to=%23%2Fs%2Fposts");
  assert.equal(links[1].getAttribute("href"), "https://example.org/admin/api/login/google?to=%23%2Fs%2Fposts");

  // Back from the provider, signed in (a new page): the header signs out.
  signedIn = true;
  document.body.innerHTML = `<div id="app" class="loading">Loading the editor…</div>`;
  const { start } = await import("../../assets/admin/cms.js");
  await start();
  const button = await until(() => $$("header button").find((b) => text(b) === "Sign out"));
  let reloaded = false;
  window.location.reload = () => {
    reloaded = true;
  };
  button.click();
  await until(() => reloaded);
  assert.deepEqual(sent.at(-1), { name: "logout", body: {} });
});
