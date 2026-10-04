/*! The browser editor of the site generator's [cms]. It bundles yaml (ISC, Eemeli Aro),
 * smol-toml (BSD-3-Clause, Squirrel Chat et al.) and marked (MIT, MarkedJS and Christopher
 * Jeffrey; Markdown BSD-style, John Gruber); their licences ship with the generator in
 * THIRD_PARTY/cms-editor/. */

// The CMS editor: pages by section, a form for their front matter, their text with a preview,
// bundle files, drafts and publishing. Everything goes through the API (`api/`, next to this
// page), the content index included (it lists drafts, so only signed-in people get it); the API
// checks every change again.
//
// The module also exports the codec and the shared helpers (for the tests); it starts the
// editor only on its page (`#app`).

import { marked } from "marked";
import { type FrontMatter, type Parts, clone, create, decode, encode, isTomlDate, join, split, tomlDate } from "./codec";
import { type Limits, base64ToText, bytesToBase64, extensionOf, mayEdit } from "./common";

export * from "./codec";
export * from "./common";

// ── The data of the API ────────────────────────────────────────────────────────────────────────

interface Lang {
  key: string;
  name: string;
  content_dir?: string;
}

interface Taxonomy {
  plural: string;
  singular: string;
  hierarchical: boolean;
  terms: string[];
}

interface FieldHint {
  label?: string;
  widget?: string;
  options?: string[];
  help?: string;
}

interface Section {
  key: string;
  title: string;
  count: number;
  style: { bundle: boolean; lang_suffix: boolean; format: string; ext: string };
  keys: { key: string; kind: string }[];
}

interface EntryFile {
  lang: string;
  path: string;
  format?: string;
  title?: string;
  draft?: boolean;
  /** A file of a page the editor is making (not saved yet). */
  doc?: Doc;
}

interface Entry {
  key: string;
  section: string;
  kind: string;
  bundle: boolean;
  title: string;
  files: EntryFile[];
  resources: string[];
  isNew?: boolean;
}

/** The content index (`GET site`, `crates/cms/src/index.rs`). */
interface Site {
  title: string;
  site_url: string;
  workflow: string;
  languages: Lang[];
  default_language: string;
  taxonomies: Taxonomy[];
  fields: Record<string, FieldHint>;
  content_dir: string | null;
  media: string | null;
  media_ref: string | null;
  upload_types: string[];
  max_upload: number;
  sections: Section[];
  entries: Entry[];
}

/** The signed-in person (`GET me`). */
interface Me extends Limits {
  email: string;
  roles: string[];
  edit: string[];
  publish: boolean;
  workflow: string;
}

interface Author {
  name: string;
  email: string;
}

interface Draft {
  id: string;
  entry: string;
  title: string;
  author: Author | null;
  updated?: string;
}

interface DraftDetail {
  id: string;
  entry: string;
  title: string;
  files: { path: string; status: string; previous?: string; patch?: string }[];
  commits: { author: Author | null; subject: string }[];
  conflicts: string[];
}

/** One language's file of the open page. */
interface Doc {
  path: string;
  /** The blob id it was loaded at (null: a new file). */
  sha: string | null;
  isNew: boolean;
  parts: Parts;
  /** The front matter as loaded, and as edited (null when it could not be read). */
  original: FrontMatter | null;
  data: FrontMatter | null;
  body: string;
  error: string | null;
  /** Edited as text. */
  raw: boolean;
  rawText: string;
  /** The text as loaded. */
  initial: string;
}

interface Upload {
  path: string;
  content: string;
}

/** The open page. */
interface Page {
  entry: Entry;
  draft: Draft | null;
  docs: Map<string, Doc>;
  uploads: Upload[];
  deletes: Set<string>;
  lang: string;
}

interface Change {
  path: string;
  content?: string;
  encoding?: "utf-8" | "base64";
  delete?: true;
  base?: string | null;
}

let site!: Site;
let me!: Me;
let drafts: Draft[] = [];
let page: Page | null = null;
/** Pages made in the editor and not saved yet, by key. */
const pending = new Map<string, Entry>();

// ── DOM helpers ────────────────────────────────────────────────────────────────────────────────

type Child = Node | string | number | null | undefined | false | Child[];

/** An element: `h("a", {href, onclick}, "text", child)`. Strings are text, never HTML. */
function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, unknown> | null = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs ?? {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v as EventListener);
    else if (k === "class") el.className = String(v);
    else if (k === "value") (el as unknown as HTMLInputElement).value = String(v);
    else if (k === "checked") (el as unknown as HTMLInputElement).checked = Boolean(v);
    else el.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of (children as unknown[]).flat(Infinity)) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

/** The value of the input an event came from. */
const valueOf = (ev: Event) => (ev.target as HTMLInputElement).value;

function app(): HTMLElement {
  const el = document.getElementById("app");
  if (!el) throw new Error("no #app");
  return el;
}

function render(...children: Child[]): void {
  const root = app();
  root.className = "";
  root.replaceChildren(header(), h("div", { class: "layout" }, sidebar(), h("main", {}, ...children)));
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;
function toast(message: string, kind: "info" | "ok" | "error" = "info"): void {
  let el = document.getElementById("toast");
  if (!el) {
    el = h("div", { id: "toast", role: "status" });
    document.body.append(el);
  }
  const box = el;
  box.className = `toast ${kind}`;
  box.textContent = message;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (box.className = "toast hidden"), kind === "error" ? 9000 : 4000);
}

// ── API ────────────────────────────────────────────────────────────────────────────────────────

/** An error answer of the API, with its JSON body (`stale`, `conflicts`). */
class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly data: Record<string, unknown>,
  ) {
    super(message);
  }
}

/** The API's URL: `api/` in the editor's directory. */
const apiBase = () => new URL("api/", new URL(".", location.href));

async function api<T>(method: "GET" | "POST", name: string, params?: Record<string, string | undefined> | null, body?: unknown): Promise<T> {
  const url = new URL(name, apiBase());
  for (const [k, v] of Object.entries(params ?? {})) if (v) url.searchParams.set(k, v);
  const res = await fetch(url, {
    method,
    credentials: "same-origin",
    headers: body ? { "content-type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  let data: Record<string, unknown> = {};
  try {
    data = await res.json();
  } catch {
    if (!res.ok) throw new ApiError(`the editor's API answered HTTP ${res.status}: is the Worker deployed?`, res.status, {});
  }
  if (!res.ok) throw new ApiError(typeof data.error === "string" ? data.error : `HTTP ${res.status}`, res.status, data);
  return data as T;
}

const messageOf = (e: unknown) => (e instanceof Error ? e.message : String(e));

async function loadDrafts(): Promise<void> {
  drafts = me.workflow === "review" ? (await api<{ drafts: Draft[] }>("GET", "drafts")).drafts : [];
}

const draftOf = (key: string) => drafts.find((d) => d.entry === key) ?? null;
const canEdit = (path: string) => mayEdit(me, me.edit, path);
const langName = (key: string) => site.languages.find((l) => l.key === key)?.name ?? key;

function current(): Page {
  if (!page) throw new Error("no page is open");
  return page;
}

// ── Layout ─────────────────────────────────────────────────────────────────────────────────────

function header(): HTMLElement {
  return h(
    "header",
    {},
    h("a", { class: "brand", href: "#/" }, site.title),
    h("a", { class: "site", href: site.site_url, target: "_blank", rel: "noopener" }, "View site ↗"),
    h("span", { class: "spacer" }),
    me.workflow === "review" ? h("a", { href: "#/drafts", class: "drafts-link" }, `Drafts (${drafts.length})`) : null,
    h("span", { class: "user", title: `Roles: ${me.roles.join(", ")}` }, me.email),
  );
}

function sidebar(): HTMLElement {
  const selected = decodeURIComponent(/^#\/s\/(.*)$/.exec(location.hash)?.[1] ?? "");
  return h(
    "nav",
    { class: "sidebar" },
    h("h2", {}, "Sections"),
    h(
      "ul",
      {},
      site.sections.map((s) =>
        h(
          "li",
          {},
          h(
            "a",
            { href: `#/s/${encodeURIComponent(s.key)}`, class: s.key === selected ? "active" : undefined },
            s.title,
            h("span", { class: "count" }, String(s.count)),
          ),
        ),
      ),
    ),
  );
}

// ── Views ──────────────────────────────────────────────────────────────────────────────────────

function home(): void {
  const mine = drafts.filter((d) => d.author?.email?.toLowerCase() === me.email);
  render(
    h("h1", {}, site.title),
    h(
      "p",
      { class: "muted" },
      `Signed in as ${me.email} (${me.roles.join(", ")}). `,
      me.workflow === "review"
        ? me.publish
          ? "Your changes are saved as drafts; you can publish drafts."
          : "Your changes are saved as drafts; someone who may publish puts them on the site."
        : "Your changes go to the site when you save them.",
    ),
    mine.length ? [h("h2", {}, "Your drafts"), draftList(mine)] : null,
    h("h2", {}, "Sections"),
    h(
      "div",
      { class: "cards" },
      site.sections.map((s) =>
        h("a", { class: "card", href: `#/s/${encodeURIComponent(s.key)}` }, h("strong", {}, s.title), h("span", { class: "muted" }, `${s.count} pages`)),
      ),
    ),
  );
}

function sectionView(key: string): void {
  const section = site.sections.find((s) => s.key === key);
  if (!section) return notFound();
  const filter = h("input", { type: "search", placeholder: "Filter by title or path", class: "filter" });
  const entries = site.entries
    .filter((e) => e.section === key)
    .sort((a, b) => (a.kind === b.kind ? a.title.localeCompare(b.title) : a.kind === "page" ? 1 : -1));
  const rows = entries.map((e) => {
    const draft = draftOf(e.key);
    return h(
      "tr",
      { "data-text": `${e.title} ${e.key}`.toLowerCase() },
      h("td", {}, h("a", { href: `#/e/${encodeURIComponent(e.key)}` }, e.title), e.kind !== "page" ? h("span", { class: "badge" }, "section page") : null),
      h(
        "td",
        {},
        e.files.map((f) => h("span", { class: `badge lang${f.draft ? " draft" : ""}`, title: f.draft ? "draft: true" : f.path }, f.lang)),
      ),
      h("td", {}, draft ? h("a", { class: "badge pending", href: `#/d/${draft.id}` }, "draft") : null),
    );
  });
  const body = h("tbody", {}, rows);
  const table = h(
    "table",
    { class: "entries" },
    h("thead", {}, h("tr", {}, h("th", {}, "Title"), h("th", {}, "Languages"), h("th", {}, ""))),
    body,
  );
  filter.addEventListener("input", () => {
    const q = filter.value.trim().toLowerCase();
    for (const tr of Array.from(body.rows)) tr.hidden = q !== "" && !(tr.dataset.text ?? "").includes(q);
  });
  render(
    h("div", { class: "title-row" }, h("h1", {}, section.title), h("button", { class: "primary", onclick: () => newPage(section) }, "New page")),
    filter,
    table,
  );
}

function notFound(): void {
  render(h("h1", {}, "Not found"), h("p", {}, h("a", { href: "#/" }, "Back to the start")));
}

// ── Pages ──────────────────────────────────────────────────────────────────────────────────────

/** A document of the open page: one language's file. */
function makeDoc(path: string, text: string, sha: string | null, isNew = false): Doc {
  const parts = split(text);
  const doc: Doc = { path, sha, isNew, parts, original: null, data: null, body: parts.body, error: null, raw: false, rawText: text, initial: text };
  try {
    doc.original = decode(parts);
    doc.data = clone(doc.original);
  } catch (e) {
    doc.error = messageOf(e);
    doc.raw = true;
  }
  return doc;
}

/** The file text of a document as it is now. */
function docText(doc: Doc): string {
  if (doc.raw || doc.original === null || doc.data === null) return doc.rawText;
  return join({ ...encode(doc.parts, doc.original, doc.data), body: doc.body });
}

async function openPage(key: string): Promise<void> {
  const entry = site.entries.find((e) => e.key === key) ?? pending.get(key);
  if (!entry) return notFound();
  render(h("p", { class: "muted" }, "Loading…"));
  const draft = draftOf(key);
  const docs = new Map<string, Doc>();
  try {
    await Promise.all(
      entry.files.map(async (f) => {
        if (f.doc) {
          docs.set(f.lang, f.doc);
          return;
        }
        try {
          const file = await api<{ content: string; sha: string }>("GET", "file", { path: f.path, draft: draft?.id });
          docs.set(f.lang, makeDoc(f.path, base64ToText(file.content), file.sha));
        } catch (e) {
          if (!(e instanceof ApiError) || e.status !== 404) throw e;
        }
      }),
    );
  } catch (e) {
    toast(messageOf(e), "error");
  }
  const langs = site.languages.map((l) => l.key).filter((l) => docs.has(l));
  page = { entry, draft, docs, uploads: [], deletes: new Set(), lang: langs[0] ?? site.default_language };
  pageView();
}

function pageView(): void {
  const p = current();
  const { entry, draft, docs } = p;
  const section = site.sections.find((s) => s.key === entry.section);
  const doc = docs.get(p.lang);
  const missing = site.languages.filter((l) => !docs.has(l.key));
  const editable = [...docs.values()].some((d) => canEdit(d.path));
  const title = doc?.data?.title;
  render(
    h("div", { class: "crumbs" }, h("a", { href: `#/s/${encodeURIComponent(entry.section)}` }, section?.title ?? "Pages"), " / ", entry.key),
    h(
      "div",
      { class: "title-row" },
      h("h1", {}, typeof title === "string" && title ? title : entry.title),
      draft ? h("a", { class: "badge pending", href: `#/d/${draft.id}` }, "has a draft") : null,
    ),
    h(
      "div",
      { class: "tabs" },
      [...docs.keys()].map((lang) =>
        h(
          "button",
          {
            class: lang === p.lang ? "tab active" : "tab",
            onclick: () => {
              p.lang = lang;
              pageView();
            },
          },
          langName(lang),
        ),
      ),
      missing.map((l) => h("button", { class: "tab add", onclick: () => addTranslation(l.key), title: `Add a ${l.name} version` }, `+ ${l.name}`)),
    ),
    doc ? docEditor(doc) : h("p", { class: "muted" }, "This page has no file in this language yet."),
    entry.bundle || site.media ? filesPanel() : null,
    h(
      "div",
      { class: "actions sticky" },
      editable
        ? h("button", { class: "primary", onclick: save }, me.workflow === "review" ? "Save draft" : "Save and publish")
        : h("span", { class: "muted" }, "You may not change this page."),
      draft && me.publish ? h("button", { onclick: () => publish(draft.id) }, "Publish draft") : null,
      draft ? h("button", { class: "danger", onclick: () => discard(draft.id) }, "Discard draft") : null,
      editable && !entry.isNew ? h("button", { class: "danger subtle", onclick: deletePage }, "Delete page") : null,
    ),
  );
}

function docEditor(doc: Doc): HTMLElement {
  const readOnly = !canEdit(doc.path);
  const head = h(
    "div",
    { class: "doc-head" },
    h("code", {}, doc.path),
    readOnly ? h("span", { class: "badge" }, "read only") : null,
    h("span", { class: "spacer" }),
    h(
      "label",
      { class: "toggle" },
      h("input", {
        type: "checkbox",
        checked: doc.raw,
        disabled: readOnly || (doc.error !== null && doc.raw) ? true : undefined,
        onchange: (ev: Event) => toggleRaw(doc, (ev.target as HTMLInputElement).checked),
      }),
      " Edit as text",
    ),
  );
  if (doc.raw || doc.data === null) {
    return h(
      "section",
      { class: "doc" },
      head,
      doc.error ? h("p", { class: "warn" }, `The front matter could not be read (${doc.error}); edit the file as text.`) : null,
      h("textarea", {
        class: "raw",
        rows: 30,
        spellcheck: "false",
        readonly: readOnly || undefined,
        value: doc.rawText,
        oninput: (ev: Event) => {
          doc.rawText = valueOf(ev);
        },
      }),
    );
  }
  return h("section", { class: "doc" }, head, fieldsForm(doc.data, readOnly), bodyEditor(doc, readOnly));
}

function toggleRaw(doc: Doc, raw: boolean): void {
  if (raw) {
    doc.rawText = docText({ ...doc, raw: false });
    doc.raw = true;
  } else {
    const next = makeDoc(doc.path, doc.rawText, doc.sha, doc.isNew);
    if (next.error) {
      toast(`The front matter has an error: ${next.error}`, "error");
      return pageView();
    }
    // The original stays as it was loaded, so unchanged keys keep their text.
    Object.assign(doc, { parts: next.parts, data: next.data, body: next.body, raw: false, error: null });
    if (doc.original === null) doc.original = next.original;
  }
  pageView();
}

// ── Front matter form ──────────────────────────────────────────────────────────────────────────

const taxonomies = () => new Map(site.taxonomies.map((t) => [t.plural.toLowerCase(), t]));
const DATE_KEYS = new Set(["date", "publishdate", "pubdate", "published", "lastmod", "modified", "expirydate", "unpublishdate"]);
const looksLikeDate = (v: unknown) =>
  typeof v === "string" && /^\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?)?$/.test(v);

/** Stores a field's new value. */
type Setter = (value: unknown) => void;

function hintOf(key: string): FieldHint {
  return site.fields?.[key.toLowerCase()] ?? {};
}

function fieldsForm(data: FrontMatter, readOnly: boolean): HTMLElement {
  const section = site.sections.find((s) => s.key === current().entry.section);
  const absent = (section?.keys ?? []).filter((k) => !Object.keys(data).some((x) => x.toLowerCase() === k.key.toLowerCase()));
  const form = h(
    "div",
    { class: "fields" },
    Object.keys(data).map((key) => {
      const hint = hintOf(key);
      if (hint.widget === "hidden") return null;
      return field(
        key,
        hint.label ?? key,
        data[key],
        (v) => {
          data[key] = v;
        },
        readOnly,
        [key],
        () => {
          delete data[key];
          pageView();
        },
      );
    }),
  );
  if (!readOnly && absent.length) {
    const select = h("select", {}, h("option", { value: "" }, "Add a field…"), absent.map((k) => h("option", { value: k.key }, k.key)));
    select.addEventListener("change", () => {
      const k = absent.find((x) => x.key === select.value);
      if (!k) return;
      data[k.key] = emptyOf(k.kind);
      pageView();
    });
    form.append(h("div", { class: "add-field" }, select));
  }
  return form;
}

function emptyOf(kind: string): unknown {
  switch (kind) {
    case "boolean":
      return false;
    case "number":
      return 0;
    case "list":
      return [];
    case "objects":
      return [{}];
    case "map":
      return {};
    case "date":
      return new Date().toISOString();
    default:
      return "";
  }
}

/** One field: a label and the input for `value`. */
function field(
  key: string,
  label: string,
  value: unknown,
  set: Setter,
  readOnly: boolean,
  path: (string | number)[],
  remove: (() => void) | null,
): HTMLElement {
  const id = `f-${path.join("-")}`.replace(/[^A-Za-z0-9_-]/g, "_");
  const hint = path.length === 1 ? hintOf(key) : {};
  const input = inputFor(key, value, set, readOnly, path, hint, id);
  return h(
    "div",
    { class: `field${input.classList.contains("group") ? " wide" : ""}` },
    h("label", { for: id }, label, remove && !readOnly ? h("button", { class: "icon", title: `Remove ${key}`, onclick: remove }, "×") : null),
    input,
    hint.help ? h("small", { class: "muted" }, hint.help) : null,
  );
}

const isPlainObject = (x: unknown): x is Record<string, unknown> =>
  x !== null && typeof x === "object" && !Array.isArray(x) && !(x instanceof Date);

function inputFor(
  key: string,
  value: unknown,
  set: Setter,
  readOnly: boolean,
  path: (string | number)[],
  hint: FieldHint,
  id: string,
): HTMLElement {
  const ro = readOnly || undefined;
  const taxonomy = path.length === 1 ? taxonomies().get(key.toLowerCase()) : undefined;
  const widget = hint.widget;
  if (taxonomy && (typeof value === "string" || Array.isArray(value) || value === null)) {
    return termsInput(taxonomy, value, set, readOnly, id);
  }
  if (widget === "select" || (hint.options?.length && typeof value !== "object")) {
    const options = [...(hint.options ?? [])];
    if (value !== null && value !== undefined && value !== "" && !options.includes(String(value))) options.unshift(String(value));
    return h(
      "select",
      { id, disabled: ro, onchange: (ev: Event) => set(valueOf(ev)) },
      h("option", { value: "" }, "—"),
      options.map((o) => h("option", { value: o, selected: String(value) === o || undefined }, o)),
    );
  }
  if (widget === "image" || (path.length === 1 && /^image|_image$|^cover$|^thumbnail$/i.test(key) && typeof value === "string")) {
    return imageInput(typeof value === "string" ? value : "", set, readOnly, id);
  }
  if (typeof value === "boolean" || widget === "boolean") {
    return h("input", { id, type: "checkbox", checked: Boolean(value), disabled: ro, onchange: (ev: Event) => set((ev.target as HTMLInputElement).checked) });
  }
  if (typeof value === "number" || widget === "number") {
    const integer = Number.isInteger(value);
    return h("input", {
      id,
      type: "number",
      step: integer ? "1" : "any",
      value: value ?? "",
      readonly: ro,
      oninput: (ev: Event) => {
        const text = valueOf(ev);
        const n = text === "" ? null : Number(text);
        if (n === null || !Number.isNaN(n)) set(n);
      },
    });
  }
  if (isTomlDate(value) || widget === "date" || (typeof value === "string" && (DATE_KEYS.has(key.toLowerCase()) || looksLikeDate(value)))) {
    return dateInput(value, set, readOnly, id);
  }
  if (Array.isArray(value)) {
    return value.length && value.every(isPlainObject) ? objectsInput(key, value, set, readOnly, path) : listInput(value, set, readOnly, id);
  }
  if (isPlainObject(value)) {
    return mapInput(value, set, readOnly, path);
  }
  const text = value === null || value === undefined ? "" : String(value);
  if (widget === "textarea" || text.length > 90 || text.includes("\n")) {
    return h("textarea", {
      id,
      rows: Math.min(8, 2 + Math.floor(text.length / 90)),
      readonly: ro,
      value: text,
      oninput: (ev: Event) => set(valueOf(ev)),
    });
  }
  return h("input", {
    id,
    type: "text",
    value: text,
    readonly: ro,
    oninput: (ev: Event) => set(value === null && valueOf(ev) === "" ? null : valueOf(ev)),
  });
}

function dateInput(value: unknown, set: Setter, readOnly: boolean, id: string): HTMLElement {
  const toml = isTomlDate(value);
  const shown = toml ? value.toISOString() : typeof value === "string" ? value : "";
  const input = h("input", {
    id,
    type: "text",
    class: "date",
    value: shown,
    readonly: readOnly || undefined,
    placeholder: "2026-01-31T12:00:00Z",
    oninput: (ev: Event) => {
      if (!toml) return set(valueOf(ev));
      const d = tomlDate(valueOf(ev));
      input.classList.toggle("invalid", !d);
      if (d) set(d);
    },
  });
  const now = h(
    "button",
    {
      class: "small",
      disabled: readOnly || undefined,
      onclick: () => {
        input.value = new Date().toISOString();
        input.dispatchEvent(new Event("input"));
      },
    },
    "Now",
  );
  return h("span", { class: "row" }, input, now);
}

function termsInput(taxonomy: Taxonomy, value: unknown, set: Setter, readOnly: boolean, id: string): HTMLElement {
  const wasString = typeof value === "string";
  const items = Array.isArray(value) ? value.map(String) : value ? [String(value)] : [];
  const listId = `terms-${taxonomy.plural}`;
  const store = () => set(wasString && items.length <= 1 ? (items[0] ?? "") : [...items]);
  const wrap = h("span", { class: "terms" });
  const draw = () => {
    wrap.replaceChildren(
      ...items.map((t, i) =>
        h(
          "span",
          { class: "chip" },
          t,
          readOnly
            ? null
            : h(
                "button",
                {
                  class: "icon",
                  title: `Remove ${t}`,
                  onclick: () => {
                    items.splice(i, 1);
                    store();
                    draw();
                  },
                },
                "×",
              ),
        ),
      ),
      readOnly
        ? ""
        : h("input", {
            id,
            list: listId,
            placeholder: `Add ${taxonomy.singular}…`,
            onkeydown: (ev: KeyboardEvent) => {
              if (ev.key !== "Enter" && ev.key !== ",") return;
              ev.preventDefault();
              const t = valueOf(ev).trim();
              if (t && !items.includes(t)) {
                items.push(t);
                store();
              }
              draw();
              wrap.querySelector("input")?.focus();
            },
          }),
    );
  };
  draw();
  if (!document.getElementById(listId)) {
    document.body.append(h("datalist", { id: listId }, taxonomy.terms.map((t) => h("option", { value: t }))));
  }
  return wrap;
}

function listInput(value: unknown[], set: Setter, readOnly: boolean, id: string): HTMLElement {
  const items = [...value];
  const wrap = h("div", { class: "list" });
  const draw = () => {
    wrap.replaceChildren(
      ...items.map((item, i) =>
        h(
          "div",
          { class: "row" },
          h("input", {
            id: i === 0 ? id : undefined,
            type: "text",
            value: item === null ? "" : String(item),
            readonly: readOnly || undefined,
            oninput: (ev: Event) => {
              const text = valueOf(ev);
              items[i] = typeof item === "number" && text.trim() !== "" && !Number.isNaN(Number(text)) ? Number(text) : text;
              set([...items]);
            },
          }),
          readOnly
            ? null
            : h(
                "button",
                {
                  class: "icon",
                  title: "Remove",
                  onclick: () => {
                    items.splice(i, 1);
                    set([...items]);
                    draw();
                  },
                },
                "×",
              ),
        ),
      ),
      readOnly
        ? ""
        : h(
            "button",
            {
              class: "small",
              onclick: () => {
                items.push("");
                set([...items]);
                draw();
              },
            },
            "Add",
          ),
    );
  };
  draw();
  return wrap;
}

function mapInput(obj: Record<string, unknown>, set: Setter, readOnly: boolean, path: (string | number)[]): HTMLElement {
  return h(
    "fieldset",
    { class: "group" },
    Object.keys(obj).map((k) =>
      field(
        k,
        k,
        obj[k],
        (v) => {
          obj[k] = v;
          set(obj);
        },
        readOnly,
        [...path, k],
        null,
      ),
    ),
  );
}

function objectsInput(key: string, items: Record<string, unknown>[], set: Setter, readOnly: boolean, path: (string | number)[]): HTMLElement {
  const wrap = h("div", { class: "group objects" });
  const draw = () => {
    wrap.replaceChildren(
      ...items.map((item, i) =>
        h(
          "fieldset",
          { class: "group item" },
          h(
            "legend",
            {},
            `${key} ${i + 1}`,
            readOnly
              ? null
              : h(
                  "button",
                  {
                    class: "icon",
                    title: "Remove",
                    onclick: () => {
                      items.splice(i, 1);
                      set(items);
                      draw();
                    },
                  },
                  "×",
                ),
          ),
          Object.keys(item).map((k) =>
            field(
              k,
              k,
              item[k],
              (v) => {
                item[k] = v;
                set(items);
              },
              readOnly,
              [...path, i, k],
              null,
            ),
          ),
        ),
      ),
      readOnly
        ? ""
        : h(
            "button",
            {
              class: "small",
              onclick: () => {
                const template = Object.fromEntries(
                  Object.entries(items[0] ?? {}).map(([k, v]) => [k, typeof v === "number" ? 0 : typeof v === "boolean" ? false : Array.isArray(v) ? [] : ""]),
                );
                items.push(template);
                set(items);
                draw();
              },
            },
            `Add ${key}`,
          ),
    );
  };
  draw();
  return wrap;
}

function imageInput(value: string, set: Setter, readOnly: boolean, id: string): HTMLElement {
  const names = imageChoices();
  const select = h(
    "select",
    { id, disabled: readOnly || undefined, onchange: (ev: Event) => set(valueOf(ev)) },
    h("option", { value: "" }, "—"),
    (value && !names.includes(value) ? [value, ...names] : names).map((n) => h("option", { value: n, selected: n === value || undefined }, n)),
  );
  return h("span", { class: "row" }, select, h("small", { class: "muted" }, "Upload files below to add choices."));
}

/** What an image field may name: the bundle's files (relative to the bundle), and files of the
 * media directory (as `media_ref` names them). */
function imageChoices(): string[] {
  const p = current();
  const out: string[] = [];
  if (p.entry.bundle) {
    const dir = bundleDir();
    for (const f of [...p.entry.resources, ...p.uploads.map((u) => u.path)]) {
      if (f.startsWith(`${dir}/`) && !p.deletes.has(f)) out.push(f.slice(dir.length + 1));
    }
  }
  const media = site.media;
  const ref = site.media_ref;
  if (media && ref !== null) {
    for (const u of p.uploads) {
      if (u.path.startsWith(`${media}/`)) out.push(ref + u.path.slice(media.length + 1));
    }
  }
  return out;
}

// ── Body ───────────────────────────────────────────────────────────────────────────────────────

function bodyEditor(doc: Doc, readOnly: boolean): HTMLElement {
  const area = h("textarea", {
    class: "body",
    rows: 18,
    readonly: readOnly || undefined,
    value: doc.body,
    oninput: (ev: Event) => {
      doc.body = valueOf(ev);
    },
  });
  const frame = h("iframe", { class: "preview", sandbox: "", title: "Preview", hidden: true });
  const button = h("button", { class: "small" }, "Preview");
  button.addEventListener("click", () => {
    const show = frame.hidden;
    frame.hidden = !show;
    area.hidden = show;
    button.textContent = show ? "Edit" : "Preview";
    if (show) {
      // Sandboxed (no scripts, an origin of its own): the text may hold any HTML.
      frame.srcdoc = `<!doctype html><meta charset="utf-8"><style>body{font:16px/1.6 system-ui,sans-serif;max-width:46rem;margin:1rem auto;padding:0 1rem;color:#222}img{max-width:100%}pre{overflow:auto;background:#f4f4f4;padding:.5rem}</style>${marked.parse(doc.body, { async: false })}`;
    }
  });
  return h(
    "div",
    { class: "body-editor" },
    h(
      "div",
      { class: "doc-head" },
      h("strong", {}, "Text"),
      h("span", { class: "spacer" }),
      h("small", { class: "muted" }, "Markdown; the preview leaves out shortcodes and the site's styles"),
      button,
    ),
    area,
    frame,
  );
}

// ── Bundle files and uploads ───────────────────────────────────────────────────────────────────

function bundleDir(): string {
  const f = current().entry.files[0];
  return f.path.slice(0, f.path.lastIndexOf("/"));
}

function filesPanel(): HTMLElement | null {
  const p = current();
  const dir = p.entry.bundle ? bundleDir() : site.media;
  if (!dir) return null;
  const files = p.entry.bundle ? [...p.entry.resources] : [];
  const waiting = p.uploads.map((u) => u.path);
  const allowed = canEdit(`${dir}/upload.jpg`);
  const input = h("input", { type: "file", multiple: true, accept: site.upload_types.map((t) => `.${t}`).join(","), hidden: true });
  input.addEventListener("change", () => {
    if (input.files) void upload(Array.from(input.files), dir);
  });
  return h(
    "section",
    { class: "files" },
    h(
      "div",
      { class: "doc-head" },
      h("strong", {}, p.entry.bundle ? "Files of this page" : "Uploads"),
      h("span", { class: "spacer" }),
      allowed ? h("button", { class: "small", onclick: () => input.click() }, "Upload…") : null,
      input,
    ),
    h("p", { class: "muted" }, p.entry.bundle ? `In ${dir}/` : `Into ${dir}/ (name them as ${site.media_ref ?? `${dir}/`}…)`),
    h(
      "ul",
      { class: "file-list" },
      [...files, ...waiting].map((f) =>
        h(
          "li",
          { class: p.deletes.has(f) ? "deleted" : waiting.includes(f) ? "new" : undefined },
          h("code", {}, f.slice(dir.length + 1)),
          waiting.includes(f) ? h("span", { class: "badge" }, "not saved") : null,
          canEdit(f) && !p.deletes.has(f)
            ? h(
                "button",
                {
                  class: "icon",
                  title: "Delete",
                  onclick: () => {
                    if (waiting.includes(f)) p.uploads = p.uploads.filter((u) => u.path !== f);
                    else p.deletes.add(f);
                    pageView();
                  },
                },
                "×",
              )
            : null,
        ),
      ),
    ),
  );
}

async function upload(files: File[], dir: string): Promise<void> {
  const p = current();
  const limit = site.max_upload;
  for (const file of files) {
    const name = file.name.normalize("NFC").replace(/[\\/]/g, "-").replace(/^\.+/, "");
    const path = `${dir}/${name}`;
    if (!site.upload_types.includes(extensionOf(name))) {
      toast(`${name}: this file type cannot be uploaded`, "error");
      continue;
    }
    if (file.size > limit) {
      toast(`${name} is larger than ${Math.round(limit / 1048576)} MB`, "error");
      continue;
    }
    if (!canEdit(path)) {
      toast(`You may not add ${path}`, "error");
      continue;
    }
    const content = bytesToBase64(new Uint8Array(await file.arrayBuffer()));
    p.uploads = p.uploads.filter((u) => u.path !== path);
    p.uploads.push({ path, content });
  }
  pageView();
}

// ── New pages, translations, deleting ──────────────────────────────────────────────────────────

const slugify = (s: string) =>
  s
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);

/** The path of a page's file in `lang`, written like the section's pages. */
function pathFor(section: Section, key: string, lang: string, bundle: boolean): string {
  const style = section.style;
  const langDir = site.languages.find((l) => l.key === lang)?.content_dir;
  const root = langDir ?? site.content_dir ?? "content";
  const suffix = style.lang_suffix || (!langDir && lang !== site.default_language) ? `.${lang}` : "";
  return bundle ? `${root}/${key}/index${suffix}.${style.ext}` : `${root}/${key}${suffix}.${style.ext}`;
}

function newPage(section: Section): void {
  const title = prompt(`Title of the new page in ${section.title}:`)?.trim();
  if (!title) return;
  const slug = slugify(title) || `page-${Date.now()}`;
  const key = section.key ? `${section.key}/${slug}` : slug;
  if (site.entries.some((e) => e.key === key)) {
    toast(`A page ${key} exists already`, "error");
    return;
  }
  const lang = site.default_language;
  const path = pathFor(section, key, lang, section.style.bundle);
  if (!canEdit(path)) {
    toast(`You may not create ${path}`, "error");
    return;
  }
  const data: FrontMatter = { title, date: new Date().toISOString() };
  const terms = taxonomies();
  for (const k of section.keys) {
    if (Object.keys(data).some((x) => x.toLowerCase() === k.key.toLowerCase())) continue;
    // Taxonomy keys start as empty lists (an empty string could name a term).
    if (terms.has(k.key.toLowerCase())) data[k.key] = [];
    else if (["string", "list", "boolean"].includes(k.kind)) data[k.key] = emptyOf(k.kind);
  }
  const doc = makeDoc(path, join(create(section.style.format, data, "")), null, true);
  pending.set(key, { key, section: section.key, kind: "page", bundle: section.style.bundle, title, resources: [], isNew: true, files: [{ lang, path, doc }] });
  location.hash = `#/e/${encodeURIComponent(key)}`;
}

function addTranslation(lang: string): void {
  const p = current();
  const section = site.sections.find((s) => s.key === p.entry.section) ?? site.sections[0];
  const from = p.docs.get(site.default_language) ?? [...p.docs.values()][0];
  const first = p.entry.files[0];
  let path: string;
  if (first && site.languages.some((l) => first.path.includes(`.${l.key}.`))) {
    path = first.path.replace(new RegExp(`\\.${first.lang}\\.([^./]+)$`), `.${lang}.$1`);
  } else {
    path = pathFor(section, p.entry.key.replace(/\/_index$/, ""), lang, p.entry.bundle);
    if (p.entry.kind !== "page") path = path.replace(/\/index(\.[^/]+)$/, "/_index$1");
  }
  if (!canEdit(path)) {
    toast(`You may not create ${path}`, "error");
    return;
  }
  const text = from ? docText(from) : join(create(section.style.format, { title: p.entry.title }, ""));
  p.docs.set(lang, makeDoc(path, text, null, true));
  p.lang = lang;
  pageView();
}

async function deletePage(): Promise<void> {
  const p = current();
  const paths = [...p.docs.values()]
    .filter((d) => !d.isNew)
    .map((d) => d.path)
    .concat(p.entry.resources);
  if (!paths.every(canEdit)) {
    toast("You may not delete every file of this page", "error");
    return;
  }
  if (!confirm(`Delete ${p.entry.title} (${paths.length} files)?`)) return;
  const sha = new Map([...p.docs.values()].map((d) => [d.path, d.sha]));
  await send(paths.map((f): Change => (sha.has(f) ? { path: f, delete: true, base: sha.get(f) } : { path: f, delete: true })));
}

// ── Saving, publishing ─────────────────────────────────────────────────────────────────────────

async function save(): Promise<void> {
  const p = current();
  const changes: Change[] = [];
  for (const doc of p.docs.values()) {
    if (!canEdit(doc.path)) continue;
    let text: string;
    try {
      text = docText(doc);
    } catch (e) {
      toast(`${doc.path}: ${messageOf(e)}`, "error");
      return;
    }
    if (!doc.isNew && text === doc.initial) continue;
    changes.push({ path: doc.path, content: text, encoding: "utf-8", base: doc.sha });
  }
  for (const u of p.uploads) changes.push({ path: u.path, content: u.content, encoding: "base64" });
  for (const f of p.deletes) changes.push({ path: f, delete: true });
  if (changes.length === 0) {
    toast("Nothing changed");
    return;
  }
  await send(changes);
}

async function send(changes: Change[]): Promise<void> {
  const p = current();
  const title = p.docs.get(site.default_language)?.data?.title ?? p.entry.title;
  try {
    const res = await api<{ draft: string | null }>("POST", "save", null, { entry: p.entry.key, title: String(title ?? ""), changes });
    toast(res.draft ? "Saved as a draft" : "Saved: the site updates after its next build", "ok");
    pending.delete(p.entry.key);
    const files = [...p.docs.entries()].map(([lang, d]): EntryFile => ({ lang, path: d.path, format: d.parts.format, title: String(d.data?.title ?? "") }));
    const known = site.entries.find((e) => e.key === p.entry.key);
    if (!known) {
      site.entries.push({ ...p.entry, isNew: undefined, files, resources: p.uploads.map((u) => u.path) });
    } else {
      known.files = files;
      known.resources = [...new Set([...known.resources, ...p.uploads.map((u) => u.path)])].filter((f) => !p.deletes.has(f));
    }
    await loadDrafts();
    await openPage(p.entry.key);
  } catch (e) {
    const stale = e instanceof ApiError ? e.data.stale : undefined;
    toast(Array.isArray(stale) ? `${messageOf(e)}: ${stale.join(", ")}` : messageOf(e), "error");
  }
}

async function publish(id: string): Promise<void> {
  if (!confirm("Publish this draft to the site?")) return;
  try {
    const res = await api<{ published: boolean; kept?: boolean }>("POST", "publish", null, { id });
    toast(
      res.kept
        ? "Published; changes saved during publishing stay in the draft"
        : res.published === false
          ? "The draft had no changes; it is gone"
          : "Published: the site updates after its next build",
      "ok",
    );
    await loadDrafts();
    route();
  } catch (e) {
    const conflicts = e instanceof ApiError ? e.data.conflicts : undefined;
    toast(Array.isArray(conflicts) ? `${messageOf(e)}: ${conflicts.join(", ")}` : messageOf(e), "error");
  }
}

async function discard(id: string): Promise<void> {
  if (!confirm("Discard this draft? Its changes are lost.")) return;
  try {
    await api("POST", "discard", null, { id });
    toast("Draft discarded", "ok");
    await loadDrafts();
    route();
  } catch (e) {
    toast(messageOf(e), "error");
  }
}

// ── Drafts ─────────────────────────────────────────────────────────────────────────────────────

function draftList(list: Draft[]): HTMLElement {
  if (list.length === 0) return h("p", { class: "muted" }, "No drafts.");
  return h(
    "table",
    { class: "entries" },
    h(
      "tbody",
      {},
      list.map((d) =>
        h(
          "tr",
          {},
          h("td", {}, h("a", { href: `#/d/${d.id}` }, d.title || d.entry)),
          h("td", { class: "muted" }, d.author?.email ?? ""),
          h("td", { class: "muted" }, d.updated ? new Date(d.updated).toLocaleString() : ""),
        ),
      ),
    ),
  );
}

function draftsView(): void {
  render(h("h1", {}, "Drafts"), draftList(drafts));
}

async function draftView(id: string): Promise<void> {
  render(h("p", { class: "muted" }, "Loading…"));
  let d: DraftDetail;
  try {
    d = await api<DraftDetail>("GET", "draft", { id });
  } catch (e) {
    toast(messageOf(e), "error");
    return draftsView();
  }
  const known = site.entries.some((e) => e.key === d.entry) || pending.has(d.entry);
  render(
    h("div", { class: "crumbs" }, h("a", { href: "#/drafts" }, "Drafts"), " / ", d.entry),
    h("h1", {}, d.title || d.entry),
    d.conflicts.length
      ? h("p", { class: "warn" }, `The site changed ${d.conflicts.join(", ")} since this draft was made: open the page, redo the changes, and discard this draft.`)
      : null,
    h("h2", {}, "Changes"),
    d.files.map((f) =>
      h(
        "details",
        { class: "change", open: d.files.length <= 3 || undefined },
        h("summary", {}, h("span", { class: `badge ${f.status}` }, f.status), " ", h("code", {}, f.previous ? `${f.previous} → ${f.path}` : f.path)),
        f.patch ? diff(f.patch) : h("p", { class: "muted" }, "A binary file, or too large to show."),
      ),
    ),
    h("h2", {}, "Saves"),
    h(
      "ul",
      {},
      d.commits.map((c) => h("li", {}, h("span", { class: "muted" }, c.author?.email ?? ""), " — ", c.subject)),
    ),
    h(
      "div",
      { class: "actions" },
      known ? h("a", { class: "button", href: `#/e/${encodeURIComponent(d.entry)}` }, "Open the page") : null,
      me.publish ? h("button", { class: "primary", disabled: d.conflicts.length > 0 || undefined, onclick: () => publish(id) }, "Publish") : null,
      h("button", { class: "danger", onclick: () => discard(id) }, "Discard"),
    ),
  );
}

function diff(patch: string): HTMLElement {
  return h(
    "pre",
    { class: "diff" },
    patch
      .split("\n")
      .map((line) =>
        h("span", { class: line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : line.startsWith("@@") ? "hunk" : undefined }, `${line}\n`),
      ),
  );
}

// ── Routing ────────────────────────────────────────────────────────────────────────────────────

function route(): void {
  const hash = location.hash;
  let m: RegExpExecArray | null;
  if ((m = /^#\/s\/(.*)$/.exec(hash))) return sectionView(decodeURIComponent(m[1]));
  if ((m = /^#\/e\/(.+)$/.exec(hash))) return void openPage(decodeURIComponent(m[1]));
  if (hash === "#/drafts") return draftsView();
  if ((m = /^#\/d\/([a-z0-9-]+)$/.exec(hash))) return void draftView(m[1]);
  return home();
}

/** Loads the index and the signed-in person, then shows the page the address names. */
export async function start(): Promise<void> {
  try {
    site = await api<Site>("GET", "site");
    me = await api<Me>("GET", "me");
    await loadDrafts();
  } catch (e) {
    app().className = "";
    app().replaceChildren(
      h("h1", {}, document.title),
      h("p", { class: "warn" }, messageOf(e)),
      h("p", { class: "muted" }, "The editor needs its API (the site's Worker) and a sign-in through Cloudflare Access."),
    );
    return;
  }
  window.addEventListener("hashchange", route);
  window.addEventListener("beforeunload", (ev) => {
    const changed = (d: Doc) => {
      try {
        return d.isNew || docText(d) !== d.initial;
      } catch {
        return true;
      }
    };
    if (page && (page.uploads.length || page.deletes.size || [...page.docs.values()].some(changed))) ev.preventDefault();
  });
  route();
}

if (typeof document !== "undefined" && document.getElementById("app")) void start();
