// The front matter form: a field per key, with the input its field (`hints.ts`) and its value
// call for. The inputs for dates, terms, lists and images are in `inputs.ts`, tables in
// `groups.ts`.

import { html, nothing, type TemplateResult } from "lit-html";
import { live } from "lit-html/directives/live.js";
import { repeat } from "lit-html/directives/repeat.js";
import { type FrontMatter, isTomlDate } from "../codec";
import { type FieldHint, type Taxonomy } from "./data";
import { valueOf } from "./dom";
import { mapInput, objectsInput } from "./groups";
import { hintOf, isEmpty, type Path, share, startOf } from "./hints";
import { choicesInput, dateInput, imageInput, listInput, optionsOf, termsInput, valuesList } from "./inputs";
import { pageView } from "./pages";
import { current, site } from "./state";

export const taxonomies = () => new Map(site.taxonomies.map((t) => [t.plural.toLowerCase(), t]));
const DATE_KEYS = new Set(["date", "publishdate", "pubdate", "published", "lastmod", "modified", "expirydate", "unpublishdate"]);
const looksLikeDate = (v: unknown) =>
  typeof v === "string" && /^\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?)?$/.test(v);

/** Stores a field's new value. */
export type Setter = (value: unknown) => void;

export function fieldsForm(data: FrontMatter, readOnly: boolean): TemplateResult {
  const section = site.sections.find((s) => s.key === current().entry.section);
  // The keys other pages of the section use, and those only the settings name.
  const configured = Object.entries(site.fields ?? {})
    .filter(([key, h]) => h.unused && h.widget !== "hidden" && !key.includes("."))
    .map(([key, h]) => ({ key: h.name ?? key, kind: h.kind ?? "string", default: undefined as unknown }));
  const has = (key: string) => Object.keys(data).some((x) => x.toLowerCase() === key.toLowerCase());
  const absent = [...(section?.keys ?? []), ...configured].filter(
    (k, i, all) => !has(k.key) && all.findIndex((x) => x.key.toLowerCase() === k.key.toLowerCase()) === i,
  );
  // A key whose value is one for every language goes to each language's file.
  const store = (key: string, value: unknown) => {
    if (value === undefined) delete data[key];
    else data[key] = value;
    if (hintOf(key).shared) share(data, key, value);
  };
  const add = (ev: Event) => {
    const select = ev.target as HTMLSelectElement;
    const k = absent.find((x) => x.key === select.value);
    // Back to "Add a field…": the list is drawn again without the key.
    select.value = "";
    if (!k) return;
    store(k.key, startOf(k.key, k.kind, k.default));
    pageView();
  };
  const remove = (key: string) => () => {
    store(key, undefined);
    pageView();
  };
  return html`
    <div class="fields">
      ${repeat(
        Object.keys(data).filter((key) => hintOf(key).widget !== "hidden"),
        (key) => key,
        (key) => field(key, hintOf(key).label ?? key, data[key], (v) => store(key, v), readOnly, [key], remove(key)),
      )}
      ${!readOnly && absent.length
        ? html`
            <div class="add-field">
              <select @change=${add}>
                <option value="">Add a field…</option>
                ${absent.map((k) => html`<option value=${k.key}>${hintOf(k.key).label ?? k.key}</option>`)}
              </select>
            </div>
          `
        : nothing}
    </div>
  `;
}

/** One field: a label and the input for `value`. */
export function field(key: string, label: string, value: unknown, set: Setter, readOnly: boolean, path: Path, remove: (() => void) | null): TemplateResult {
  const id = `f-${path.join("-")}`.replace(/[^A-Za-z0-9_-]/g, "_");
  const hint = hintOf(path);
  const kind = kindOf(key, value, path, hint);
  const everyLanguage = path.length === 1 && hint.shared && current().docs.size > 1;
  return html`
    <div class=${kind === "map" || kind === "objects" ? "field wide" : "field"}>
      <label for=${id}>
        <span>
          ${label}${hint.required ? html`<span class="required" title="Needs a value">*</span>` : nothing}
          ${everyLanguage ? html`<span class="badge" title="One value for every language: a change goes to each">all languages</span>` : nothing}
        </span>
        ${remove && !readOnly ? html`<button class="icon" title="Remove ${key}" @click=${remove}>×</button>` : nothing}
      </label>
      ${inputFor(kind, key, value, set, readOnly, path, hint, id)}
      ${hint.help ? html`<small class="muted">${hint.help}</small>` : nothing}
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
        <select id=${id} ?disabled=${readOnly} ?required=${required} @change=${(ev: Event) => set(valueOf(ev))}>
          <option value="">—</option>
          ${optionsOf(options, String(value))}
        </select>
      `;
    }
    case "image":
      return imageInput(typeof value === "string" ? value : "", set, readOnly, id);
    case "boolean":
      return html`<input id=${id} type="checkbox" .checked=${live(Boolean(value))} ?disabled=${readOnly} @change=${(ev: Event) => set((ev.target as HTMLInputElement).checked)} />`;
    case "number": {
      const input = (ev: Event) => {
        const text = valueOf(ev);
        const n = text === "" ? null : Number(text);
        if (n === null || !Number.isNaN(n)) set(n);
      };
      const step = hint.step !== undefined ? String(hint.step) : Number.isInteger(value) ? "1" : "any";
      return html`
        <input
          id=${id}
          type="number"
          step=${step}
          min=${hint.min ?? nothing}
          max=${hint.max ?? nothing}
          ?required=${required}
          .value=${live(textOf(value))}
          ?readonly=${readOnly}
          @input=${input}
        />
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
      return html`<textarea id=${id} rows=${rows} ?readonly=${readOnly} ?required=${required} .value=${live(text)} @input=${(ev: Event) => set(valueOf(ev))}></textarea>`;
    }
    case "text": {
      // An empty value that was null stays null.
      const input = (ev: Event) => set(value === null && valueOf(ev) === "" ? null : valueOf(ev));
      const values = hint.suggestions?.length ? `${id}-values` : nothing;
      return html`
        <input id=${id} type="text" list=${values} ?required=${required} .value=${live(textOf(value))} ?readonly=${readOnly} @input=${input} />
        ${valuesList(`${id}-values`, hint.suggestions)}
      `;
    }
  }
}
