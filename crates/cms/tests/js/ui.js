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
  // The editor, lit-html and the Material components use the browser's globals: the window's
  // classes (node's own events would not reach happy-dom's listeners), where node has none or its
  // own.
  const own = ["document", "location", "Event", "CustomEvent", "KeyboardEvent", "MouseEvent", "PointerEvent", "FocusEvent", "InputEvent", "SubmitEvent", "FormData", "DOMParser", "Node", "HTMLElement"];
  for (const k of [...own, ...Object.getOwnPropertyNames(window).filter((k) => /^[A-Z]/.test(k) && !(k in globalThis))]) {
    Object.defineProperty(globalThis, k, { value: window[k], configurable: true, writable: true });
  }
  Object.defineProperty(globalThis, "customElements", { value: window.customElements, configurable: true, writable: true });
  shimMissing(window);
  Object.assign(globalThis, { window, fetch });
  document.body.innerHTML = `<div id="app" class="loading">Loading the editor…</div>`;
  await import("../../assets/admin/cms.js");
  await until(ready);
}

/**
 * Where happy-dom differs from a browser in what the Material components use:
 * - `attachInternals()`, which it lacks (capricorn86/happy-dom#1419): here internals that keep
 *   what the controls give them but join no form;
 * - an input's `type` set to `undefined`, which a browser takes as text and happy-dom throws on
 *   (a text field sets it before its own type is known);
 * - an animation cancelled: the browser rejects its `finished` promise as handled (Web Animations,
 *   "cancel an animation"), happy-dom as unhandled, which fails the test that clicked a ripple.
 */
function shimMissing(window) {
  class Internals {
    constructor() {
      Object.assign(this, { states: new Set(), form: null, labels: [], validity: { valid: true }, validationMessage: "", willValidate: true });
    }
    setFormValue(value) {
      this.value = value;
    }
    setValidity(flags = {}, message = "") {
      this.validity = { ...flags, valid: !Object.values(flags).some(Boolean) };
      this.validationMessage = message;
    }
    checkValidity() {
      return this.validity.valid;
    }
    reportValidity() {
      return this.validity.valid;
    }
  }
  window.HTMLElement.prototype.attachInternals = () => new Internals();
  const type = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "type");
  Object.defineProperty(window.HTMLInputElement.prototype, "type", { ...type, set: function (v) { type.set.call(this, String(v)); } });
  const cancel = window.Animation.prototype.cancel;
  window.Animation.prototype.cancel = function () {
    this.finished?.catch(() => {});
    return cancel.call(this);
  };
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

const BUTTONS = "button, md-filled-button, md-filled-tonal-button, md-outlined-button, md-text-button, md-icon-button";

/** The button (plain or Material) in `scope` whose text, or label, is `name`. */
export const button = (name, scope = document) =>
  [...scope.querySelectorAll(BUTTONS)].find((b) => text(b) === name || b.getAttribute("aria-label") === name);

/** Clicks `el` and lets the click's work settle. A Material button takes a second click in the
 * same microtask for the copy of the first it makes itself, and drops it: clicks by hand are
 * apart, and so are these. */
export async function press(el) {
  el.click();
  await new Promise((r) => setTimeout(r));
}

/** Answers the dialog the editor shows: types `value` into its field, if it has one, and clicks its
 * button that answers yes; `null` cancels it. Resolves once it is gone. */
export async function reply(value) {
  const dialog = await until(() => $("md-dialog[open]"));
  if (typeof value === "string") type(dialog.querySelector("md-outlined-text-field"), value);
  dialog.querySelector(value === null ? ".no" : ".yes").click();
  await until(() => !dialog.isConnected);
}

/** Chooses `label` in the menu of the add button in `scope` (Add a field, Add…). */
export function pick(scope, label) {
  const item = [...scope.querySelectorAll(".add-field md-menu-item")].find((i) => text(i) === label);
  assert.ok(item, `no ${label} in the menu`);
  item.click();
}
