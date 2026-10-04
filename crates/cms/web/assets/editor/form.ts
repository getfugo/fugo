// The front matter form: a field per key, with inputs by kind (dates, terms, lists, maps, images).

import { type FrontMatter, isTomlDate, tomlDate } from "../codec";
import { type FieldHint, type Taxonomy } from "./data";
import { h, valueOf } from "./dom";
import { bundleDir } from "./files";
import { pageView } from "./pages";
import { current, site } from "./state";

export const taxonomies = () => new Map(site.taxonomies.map((t) => [t.plural.toLowerCase(), t]));
const DATE_KEYS = new Set(["date", "publishdate", "pubdate", "published", "lastmod", "modified", "expirydate", "unpublishdate"]);
const looksLikeDate = (v: unknown) =>
  typeof v === "string" && /^\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?)?$/.test(v);

/** Stores a field's new value. */
type Setter = (value: unknown) => void;

function hintOf(key: string): FieldHint {
  return site.fields?.[key.toLowerCase()] ?? {};
}

export function fieldsForm(data: FrontMatter, readOnly: boolean): HTMLElement {
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

export function emptyOf(kind: string): unknown {
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
