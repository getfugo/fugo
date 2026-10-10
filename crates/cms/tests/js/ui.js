// The editor (assets/admin/cms.js) in happy-dom against a fake API, for the tests of its views
// (editor-*.test.js). happy-dom comes from the modules of tools/dev/node.sh, whose directory
// tests/it/js.rs passes as CMS_NODE_MODULES; without it the tests skip.

import assert from "node:assert/strict";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const modules = process.env.CMS_NODE_MODULES;
export const skip = modules ? false : "CMS_NODE_MODULES is not set (tools/dev/node.sh installs happy-dom)";

export const STYLE = { bundle: false, lang_suffix: false, format: "yaml", ext: "md" };
const ME = {
  email: "ann@example.org",
  roles: ["owner"],
  edit: ["**"],
  publish: true,
  workflow: "review",
  areas: [{ kind: "content", glob: "content/**", ext: ["md"] }],
  deny: [],
};

/** The bodies of the requests that change something, by the API's name. */
export const sent = [];
export let window, document;

/** Starts the editor on the content index `site`, whose files are `files` (path → text); `more`
 * replaces requests of the fake API (name → function of the query and the body, which may return
 * a Response), at the address `url`, and waits until `ready`. */
export async function startEditor(site, files, more = {}, { url = "https://example.org/admin/", ready = () => document.querySelector("header") } = {}) {
  const api = {
    site: () => site,
    me: () => ME,
    drafts: () => ({ drafts: [] }),
    file: (q) => (q.get("path") in files ? { content: Buffer.from(files[q.get("path")]).toString("base64"), sha: "s1" } : null),
    save: () => ({ draft: "ann-1" }),
    ...more,
  };
  const fetch = async (url, init = {}) => {
    const u = new URL(String(url), "https://example.org/admin/");
    const name = u.pathname.replace(/^\/admin\/api\//, "");
    const body = init.body ? JSON.parse(init.body) : undefined;
    if (body) sent.push({ name, body });
    const data = api[name]?.(u.searchParams, body);
    if (data instanceof Response) return data;
    return new Response(JSON.stringify(data ?? { error: "not found" }), { status: data ? 200 : 404 });
  };
  const { Window } = await import(pathToFileURL(join(modules, "happy-dom/lib/index.js")).href);
  window = new Window({ url });
  document = window.document;
  // The editor and lit-html use the browser's globals.
  for (const k of ["document", "location", "Event", "KeyboardEvent", "FormData", "DOMParser", "Node", "HTMLElement"]) {
    Object.defineProperty(globalThis, k, { value: window[k], configurable: true, writable: true });
  }
  Object.assign(globalThis, { window, fetch, confirm: () => true, prompt: () => "Honey Butter" });
  document.body.innerHTML = `<div id="app" class="loading">Loading the editor…</div>`;
  await import("../../assets/admin/cms.js");
  await until(ready);
}

/** Waits (a second at most) until `ready` is true, and returns what it returns. */
export async function until(ready) {
  for (let i = 0; i < 100; i++) {
    const v = ready();
    if (v) return v;
    await new Promise((r) => setTimeout(r, 10));
  }
  assert.fail(`not drawn: ${ready}`);
}

export async function go(hash, ready) {
  window.location.hash = hash;
  return until(ready);
}

// Never give assert an element (`assert.equal($(".x"), null)`): when the assertion fails, node
// prints its values to depth 1000 with their getters, and from an element happy-dom's getters
// reach the whole window, new objects each time: the process takes tens of gigabytes, and a
// heap limit does not stop it. Compare a count, a text, an attribute or `a === b` instead.
export const $ = (s) => document.querySelector(s);
export const $$ = (s) => [...document.querySelectorAll(s)];
export const text = (el) => el.textContent.replace(/\s+/g, " ").trim();

/** Types `value` into `input`, as if by hand. */
export function type(input, value) {
  input.value = value;
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
}

/** Chooses `value` in the select `select`. */
export function choose(select, value) {
  select.value = value;
  select.dispatchEvent(new window.Event("change", { bubbles: true }));
}
