// The front matter form: a field per key, with the input its field (`hints.ts`) and its value
// call for. The inputs for dates, terms and lists are in `inputs.ts`, images in `images.ts`,
// tables in `groups.ts`.

import { html, nothing, type TemplateResult } from "lit-html";
import { live } from "lit-html/directives/live.js";
import { repeat } from "lit-html/directives/repeat.js";
import { clone, type FrontMatter, isTomlDate } from "../codec";
import { type FieldHint, type Taxonomy } from "./data";
import { valueOf } from "./dom";
import { mapInput, objectsInput } from "./groups";
import { hintOf, isEmpty, type Path, share, startOf } from "./hints";
import { icon } from "./icons";
import { imageInput } from "./images";
import { addMenu, choicesInput, dateInput, listInput, optionsOf, termsInput, textInput, valuesList } from "./inputs";
import { type MdCheckbox } from "./material";
import { pageView } from "./pages";
import { current, langName, site } from "./state";

export const taxonomies = () => new Map(site.taxonomies.map((t) => [t.plural.toLowerCase(), t]));
const DATE_KEYS = new Set(["date", "publishdate", "pubdate", "published", "lastmod", "modified", "expirydate", "unpublishdate"]);
const looksLikeDate = (v: unknown) =>
  typeof v === "string" && /^\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?)?$/.test(v);

/** Stores a field's new value. */
export type Setter = (value: unknown) => void;

export function fieldsForm(data: FrontMatter, readOnly: boolean): TemplateResult {
  const p = current();
  const section = site.sections.find((s) => s.key === p.entry.section);
  const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  const inSection = (key: string) => section?.keys.some((k) => same(k.key, key)) ?? false;
  // The keys other pages of the section use, and those only the settings name.
  const configured = Object.entries(site.fields ?? {})
    .filter(([key, h]) => h.unused && h.widget !== "hidden" && !key.includes("."))
    .map(([key, h]) => ({ key: h.name ?? key, kind: h.kind ?? "string", default: undefined as unknown }));
  const has = (key: string) => Object.keys(data).some((x) => same(x, key));
  const absent = [...(section?.keys ?? []), ...configured].filter((k, i, all) => !has(k.key) && all.findIndex((x) => same(x.key, k.key)) === i);
  // The other languages' files of the page, the default language's first, and the value the
  // first of them that gives one gives a key.
  const others = [...p.docs.entries()]
    .sort(([a], [b]) => Number(b === site.default_language) - Number(a === site.default_language))
    .flatMap(([lang, d]) => (d.data && d.data !== data ? [{ lang, data: d.data }] : []));
  const sourceOf = (key: string) => {
    for (const o of others) {
      const k = Object.keys(o.data).find((x) => same(x, key));
      if (k !== undefined && !isEmpty(o.data[k])) return { lang: o.lang, value: o.data[k] };
    }
    return null;
  };
  // Keys the file lacks and should give, shown empty: those the settings require (on a page of a
  // section whose pages use them, as the checks before saving count them), and the text each
  // language gives its own of that another language's file has (a title to translate).
  const missing = absent.filter((k) => {
    const h = hintOf(k.key);
    const text = h.widget === "text" || h.widget === "textarea";
    return h.widget !== "hidden" && ((h.required && inSection(k.key)) || (text && !h.shared && sourceOf(k.key) !== null));
  });
  // A key the file lacks goes where the other language's file has it: before the first of the
  // keys after it there that this file has (a key that file lacks too goes first).
  const order = Object.keys(others[0]?.data ?? {}).map((k) => k.toLowerCase());
  const placeOf = (key: string, keys: string[]) => {
    const i = order.indexOf(key.toLowerCase());
    if (i < 0) return 0;
    const at = keys.findIndex((k) => order.indexOf(k.toLowerCase()) > i);
    return at < 0 ? keys.length : at;
  };
  // A key whose value is one for every language goes to each language's file.
  const store = (key: string, value: unknown) => {
    if (value === undefined) delete data[key];
    else data[key] = value;
    if (hintOf(key).shared) share(data, key, value);
  };
  // A key the file lacks goes into it, in its place, once it has a value.
  const fill = (key: string) => (value: unknown) => {
    if (has(key)) return store(key, value);
    if (isEmpty(value)) return;
    const entries = Object.entries(data);
    entries.splice(placeOf(key, Object.keys(data)), 0, [key, value]);
    for (const k of Object.keys(data)) delete data[k];
    Object.assign(data, Object.fromEntries(entries));
    if (hintOf(key).shared) share(data, key, value);
  };
  // Under a key the file lacks: another language's value, and a button that copies it here.
  const copyFrom = (key: string) => {
    const from = sourceOf(key);
    if (!from || readOnly) return nothing;
    const v = from.value;
    const shown = typeof v === "string" || typeof v === "number" || isTomlDate(v) ? String(v) : "";
    const copy = () => {
      fill(key)(clone(v));
      pageView();
    };
    return html`
      <div class="copy-from">
        <span class="muted" title=${shown}>${langName(from.lang)}${shown ? `: ${shown}` : ""}</span>
        <md-text-button @click=${copy}>Copy</md-text-button>
      </div>
    `;
  };
  const add = (key: string) => {
    const k = absent.find((x) => x.key === key);
    if (!k) return;
    store(k.key, startOf(k.key, k.kind, k.default));
    pageView();
  };
  const remove = (key: string) => () => {
    store(key, undefined);
    pageView();
  };
  const keys = Object.keys(data).filter((key) => hintOf(key).widget !== "hidden");
  const rows = keys.map((key) => ({ key, missing: false }));
  // From the last place to the first, so that each place stays where it was.
  const places = missing.map((k) => ({ key: k.key, at: placeOf(k.key, keys), rank: order.indexOf(k.key.toLowerCase()) }));
  for (const m of places.sort((a, b) => b.at - a.at || b.rank - a.rank)) rows.splice(m.at, 0, { key: m.key, missing: true });
  const more = absent.filter((k) => !missing.includes(k));
  return html`
    <div class="fields">
      ${repeat(
        rows,
        (row) => row.key,
        ({ key, missing }) =>
          missing
            ? field(key, hintOf(key).label ?? key, undefined, fill(key), readOnly, [key], null, copyFrom(key))
            : field(key, hintOf(key).label ?? key, data[key], (v) => store(key, v), readOnly, [key], remove(key)),
      )}
      ${!readOnly && more.length
        ? addMenu(
            "Add a field",
            more.map((k) => ({ key: k.key, label: hintOf(k.key).label ?? k.key })),
            add,
          )
        : nothing}
    </div>
  `;
}

/** One field: a label and the input for `value`, and `below` it. */
export function field(
  key: string,
  label: string,
  value: unknown,
  set: Setter,
  readOnly: boolean,
  path: Path,
  remove: (() => void) | null,
  below: TemplateResult | typeof nothing = nothing,
): TemplateResult {
  const id = `f-${path.join("-")}`.replace(/[^A-Za-z0-9_-]/g, "_");
  const hint = hintOf(path);
  const kind = kindOf(key, value, path, hint);
  const everyLanguage = path.length === 1 && hint.shared && current().docs.size > 1;
  return html`
    <div class=${kind === "map" || kind === "objects" ? "field wide" : "field"}>
      <div class="field-head">
        <label for=${id}>
          <span>
            ${label}${hint.required ? html`<span class="required" title="Needs a value">*</span>` : nothing}
            ${everyLanguage ? html`<span class="badge" title="One value for every language: a change goes to each">all languages</span>` : nothing}
          </span>
        </label>
        ${remove && !readOnly ? html`<md-icon-button class="remove" title="Remove ${key}" aria-label="Remove ${key}" @click=${remove}>${icon("close")}</md-icon-button>` : nothing}
      </div>
      ${inputFor(kind, key, value, set, readOnly, path, hint, id)}
      ${hint.help ? html`<small class="muted">${hint.help}</small>` : nothing}${below}
    </div>
  `;
}

const isPlainObject = (x: unknown): x is Record<string, unknown> =>
  x !== null && typeof x === "object" && !Array.isArray(x) && !(x instanceof Date);

type Kind = "terms" | "choices" | "select" | "image" | "boolean" | "number" | "date" | "objects" | "list" | "map" | "textarea" | "text";

/** The input a field gets: by its taxonomy, its field (`hint`), its value and its key. */
function kindOf(key: string, value: unknown, path: Path, hint: FieldHint): Kind {
  const widget = hint.widget;
  const top = path.length === 1;
  if (top && taxonomies().has(key.toLowerCase()) && (typeof value === "string" || Array.isArray(value) || value === null)) return "terms";
  if (widget === "select" || hint.options?.length) {
    if (hint.multiple || Array.isArray(value)) return "choices";
    if (widget === "select" || typeof value !== "object") return "select";
  }
  if (widget === "image" || (!widget && top && /^image|_image$|^cover$|^thumbnail$/i.test(key) && typeof value === "string")) return "image";
  if (typeof value === "boolean" || widget === "boolean") return "boolean";
  if (typeof value === "number" || widget === "number") return "number";
  if (isTomlDate(value) || widget === "date" || (typeof value === "string" && (DATE_KEYS.has(key.toLowerCase()) || looksLikeDate(value)))) return "date";
  if (Array.isArray(value)) return value.length && value.every(isPlainObject) ? "objects" : "list";
  if (widget === "list" && isEmpty(value)) return "list";
  if (isPlainObject(value)) return "map";
  const text = textOf(value);
  return widget === "textarea" || text.length > 90 || text.includes("\n") ? "textarea" : "text";
}

const textOf = (value: unknown) => (value === null || value === undefined ? "" : String(value));

function inputFor(kind: Kind, key: string, value: unknown, set: Setter, readOnly: boolean, path: Path, hint: FieldHint, id: string): TemplateResult {
  const required = hint.required ?? false;
  switch (kind) {
    case "terms":
      return termsInput(taxonomies().get(key.toLowerCase()) as Taxonomy, value, set, readOnly, id);
    case "choices":
      return choicesInput(hint.options ?? [], value, set, readOnly, id);
    case "select": {
      const options = [...(hint.options ?? [])];
      if (value !== null && value !== undefined && value !== "" && !options.includes(String(value))) options.unshift(String(value));
      return html`
        <md-outlined-select id=${id} ?disabled=${readOnly} ?required=${required} @change=${(ev: Event) => set(valueOf(ev))}>
          ${optionsOf(["", ...options], textOf(value))}
        </md-outlined-select>
      `;
    }
    case "image":
      return imageInput(typeof value === "string" ? value : "", set, readOnly, id);
    case "boolean":
      return html`<md-checkbox id=${id} .checked=${live(Boolean(value))} ?disabled=${readOnly} @change=${(ev: Event) => set((ev.target as MdCheckbox).checked)}></md-checkbox>`;
    case "number": {
      const input = (ev: Event) => {
        const text = valueOf(ev);
        const n = text === "" ? null : Number(text);
        if (n === null || !Number.isNaN(n)) set(n);
      };
      // Any number unless the field says its step: a value that is whole now may not stay so.
      const step = hint.step !== undefined ? String(hint.step) : "any";
      return html`
        <md-outlined-text-field
          id=${id}
          type="number"
          step=${step}
          min=${hint.min ?? nothing}
          max=${hint.max ?? nothing}
          ?required=${required}
          .value=${live(textOf(value))}
          ?readonly=${readOnly}
          @input=${input}
        ></md-outlined-text-field>
      `;
    }
    case "date":
      return dateInput(value, set, readOnly, id);
    case "objects":
      return objectsInput(key, value as Record<string, unknown>[], set, readOnly, path);
    case "list":
      return listInput(Array.isArray(value) ? value : [], set, readOnly, id, hint.suggestions);
    case "map":
      return mapInput(value as Record<string, unknown>, set, readOnly, path);
    case "textarea": {
      const text = textOf(value);
      const rows = Math.min(8, 2 + Math.floor(text.length / 90));
      return html`
        <md-outlined-text-field id=${id} type="textarea" rows=${rows} ?readonly=${readOnly} ?required=${required} .value=${live(text)} @input=${(ev: Event) => set(valueOf(ev))}></md-outlined-text-field>
      `;
    }
    case "text": {
      // An empty value that was null stays null.
      const input = (ev: Event) => set(value === null && valueOf(ev) === "" ? null : valueOf(ev));
      const values = hint.suggestions?.length ? `${id}-values` : undefined;
      return html`${textInput(id, textOf(value), input, readOnly, values, required)}${valuesList(`${id}-values`, hint.suggestions)}`;
    }
  }
}
