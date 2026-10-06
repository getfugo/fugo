// The front matter form: a field per key, with inputs by kind (dates, terms, lists, maps, images).

import { html, nothing, type TemplateResult } from "lit-html";
import { live } from "lit-html/directives/live.js";
import { repeat } from "lit-html/directives/repeat.js";
import { type FrontMatter, isTomlDate, tomlDate } from "../codec";
import { type FieldHint, type Taxonomy } from "./data";
import { valueOf } from "./dom";
import { bundleDir } from "./files";
import { pageView } from "./pages";
import { current, site } from "./state";

export const taxonomies = () => new Map(site.taxonomies.map((t) => [t.plural.toLowerCase(), t]));
const DATE_KEYS = new Set(["date", "publishdate", "pubdate", "published", "lastmod", "modified", "expirydate", "unpublishdate"]);
const looksLikeDate = (v: unknown) =>
  typeof v === "string" && /^\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?)?$/.test(v);

/** Stores a field's new value. */
type Setter = (value: unknown) => void;
/** Where a field is: its key, and the keys and indexes of the fields it is in. */
type Path = (string | number)[];

function hintOf(key: string): FieldHint {
  return site.fields?.[key.toLowerCase()] ?? {};
}

export function fieldsForm(data: FrontMatter, readOnly: boolean): TemplateResult {
  const section = site.sections.find((s) => s.key === current().entry.section);
  // The keys other pages of the section use, and those only the settings name.
  const configured = Object.entries(site.fields ?? {})
    .filter(([, h]) => h.unused && h.widget !== "hidden")
    .map(([key, h]) => ({ key, kind: h.kind ?? "string" }));
  const absent = [...(section?.keys ?? []), ...configured].filter(
    (k, i, all) => !Object.keys(data).some((x) => x.toLowerCase() === k.key.toLowerCase()) && all.findIndex((x) => x.key.toLowerCase() === k.key.toLowerCase()) === i,
  );
  const add = (ev: Event) => {
    const select = ev.target as HTMLSelectElement;
    const k = absent.find((x) => x.key === select.value);
    // Back to "Add a field…": the list is drawn again without the key.
    select.value = "";
    if (!k) return;
    data[k.key] = emptyOf(k.kind);
    pageView();
  };
  const remove = (key: string) => () => {
    delete data[key];
    pageView();
  };
  return html`
    <div class="fields">
      ${repeat(
        Object.keys(data).filter((key) => hintOf(key).widget !== "hidden"),
        (key) => key,
        (key) => field(key, hintOf(key).label ?? key, data[key], (v) => (data[key] = v), readOnly, [key], remove(key)),
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
function field(key: string, label: string, value: unknown, set: Setter, readOnly: boolean, path: Path, remove: (() => void) | null): TemplateResult {
  const id = `f-${path.join("-")}`.replace(/[^A-Za-z0-9_-]/g, "_");
  const hint = path.length === 1 ? hintOf(key) : {};
  const kind = kindOf(key, value, path, hint);
  return html`
    <div class=${kind === "map" || kind === "objects" ? "field wide" : "field"}>
      <label for=${id}>${label}${remove && !readOnly ? html`<button class="icon" title="Remove ${key}" @click=${remove}>×</button>` : nothing}</label>
      ${inputFor(kind, key, value, set, readOnly, path, hint, id)}
      ${hint.help ? html`<small class="muted">${hint.help}</small>` : nothing}
    </div>
  `;
}

const isPlainObject = (x: unknown): x is Record<string, unknown> =>
  x !== null && typeof x === "object" && !Array.isArray(x) && !(x instanceof Date);

const empty = (value: unknown) => value === null || value === undefined || value === "";

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
  if (widget === "list" && empty(value)) return "list";
  if (isPlainObject(value)) return "map";
  const text = textOf(value);
  return widget === "textarea" || text.length > 90 || text.includes("\n") ? "textarea" : "text";
}

const textOf = (value: unknown) => (value === null || value === undefined ? "" : String(value));

/** The values pages share for a key, as a `<datalist>` for inputs to suggest them (`list=`). */
const valuesList = (id: string, values: string[] | undefined) =>
  values?.length ? html`<datalist id=${id}>${values.map((v) => html`<option value=${v}></option>`)}</datalist>` : nothing;

/** The `<option>`s of `values`, `chosen` selected (also when another was chosen before a redraw). */
const optionsOf = (values: string[], chosen: string) => values.map((v) => html`<option value=${v} .selected=${live(v === chosen)}>${v}</option>`);

function inputFor(kind: Kind, key: string, value: unknown, set: Setter, readOnly: boolean, path: Path, hint: FieldHint, id: string): TemplateResult {
  switch (kind) {
    case "terms":
      return termsInput(taxonomies().get(key.toLowerCase()) as Taxonomy, value, set, readOnly, id);
    case "choices":
      return choicesInput(hint.options ?? [], value, set, readOnly, id);
    case "select": {
      const options = [...(hint.options ?? [])];
      if (value !== null && value !== undefined && value !== "" && !options.includes(String(value))) options.unshift(String(value));
      return html`
        <select id=${id} ?disabled=${readOnly} @change=${(ev: Event) => set(valueOf(ev))}>
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
      return html`<input id=${id} type="number" step=${Number.isInteger(value) ? "1" : "any"} .value=${live(textOf(value))} ?readonly=${readOnly} @input=${input} />`;
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
      return html`<textarea id=${id} rows=${rows} ?readonly=${readOnly} .value=${live(text)} @input=${(ev: Event) => set(valueOf(ev))}></textarea>`;
    }
    case "text": {
      // An empty value that was null stays null.
      const input = (ev: Event) => set(value === null && valueOf(ev) === "" ? null : valueOf(ev));
      const values = hint.suggestions?.length ? `${id}-values` : nothing;
      return html`
        <input id=${id} type="text" list=${values} .value=${live(textOf(value))} ?readonly=${readOnly} @input=${input} />
        ${valuesList(`${id}-values`, hint.suggestions)}
      `;
    }
  }
}

function dateInput(value: unknown, set: Setter, readOnly: boolean, id: string): TemplateResult {
  const toml = isTomlDate(value);
  const shown = toml ? value.toISOString() : typeof value === "string" ? value : "";
  const input = (ev: Event) => {
    const el = ev.target as HTMLInputElement;
    if (!toml) return set(el.value);
    const d = tomlDate(el.value);
    el.classList.toggle("invalid", !d);
    if (d) set(d);
  };
  // Types the time into the field, as if by hand.
  const now = (ev: Event) => {
    const el = (ev.currentTarget as HTMLElement).previousElementSibling as HTMLInputElement;
    el.value = new Date().toISOString();
    el.dispatchEvent(new Event("input", { bubbles: true }));
  };
  return html`
    <span class="row">
      <input id=${id} type="text" class="date" placeholder="2026-01-31T12:00:00Z" .value=${live(shown)} ?readonly=${readOnly} @input=${input} />
      <button class="small" ?disabled=${readOnly} @click=${now}>Now</button>
    </span>
  `;
}

function termsInput(taxonomy: Taxonomy, value: unknown, set: Setter, readOnly: boolean, id: string): TemplateResult {
  const items = Array.isArray(value) ? value.map(String) : value ? [String(value)] : [];
  const listId = `terms-${taxonomy.plural}`;
  // One term stays a string where it was one.
  const store = (next: string[]) => {
    set(typeof value === "string" && next.length <= 1 ? (next[0] ?? "") : next);
    pageView();
  };
  const keydown = (ev: KeyboardEvent) => {
    if (ev.key !== "Enter" && ev.key !== ",") return;
    ev.preventDefault();
    const input = ev.target as HTMLInputElement;
    const t = input.value.trim();
    input.value = "";
    if (t && !items.includes(t)) store([...items, t]);
  };
  const chip = (t: string, i: number) => html`
    <span class="chip">${t}${readOnly ? nothing : html`<button class="icon" title="Remove ${t}" @click=${() => store(items.filter((_, j) => j !== i))}>×</button>`}</span>
  `;
  return html`
    <span class="terms">
      ${items.map(chip)}
      ${readOnly
        ? nothing
        : html`
            <input id=${id} list=${listId} placeholder="Add ${taxonomy.singular}…" @keydown=${keydown} />
            <datalist id=${listId}>${taxonomy.terms.map((t) => html`<option value=${t}></option>`)}</datalist>
          `}
    </span>
  `;
}

/** Several options, any number of them chosen: a list of the chosen ones, in the options' order
 * (a chosen value that is not an option stays, after them). */
function choicesInput(options: string[], value: unknown, set: Setter, readOnly: boolean, id: string): TemplateResult {
  let chosen = Array.isArray(value) ? value.map(String) : empty(value) ? [] : [String(value)];
  const all = [...options, ...chosen.filter((c) => !options.includes(c))];
  const toggle = (option: string) => (ev: Event) => {
    const on = (ev.target as HTMLInputElement).checked;
    chosen = on ? [...chosen, option] : chosen.filter((c) => c !== option);
    set(all.filter((o) => chosen.includes(o)));
  };
  return html`
    <span class="choices">
      ${all.map(
        (o, i) => html`
          <label>
            <input type="checkbox" id=${i === 0 ? id : nothing} .checked=${live(chosen.includes(o))} ?disabled=${readOnly} @change=${toggle(o)} />
            ${o}
          </label>
        `,
      )}
    </span>
  `;
}

function listInput(value: unknown[], set: Setter, readOnly: boolean, id: string, suggestions?: string[]): TemplateResult {
  const items = [...value];
  const values = suggestions?.length ? `${id}-values` : nothing;
  const change = (next: unknown[]) => {
    set(next);
    pageView();
  };
  // A number stays a number while it reads as one.
  const edit = (i: number) => (ev: Event) => {
    const text = valueOf(ev);
    items[i] = typeof value[i] === "number" && text.trim() !== "" && !Number.isNaN(Number(text)) ? Number(text) : text;
    set([...items]);
  };
  const row = (item: unknown, i: number) => html`
    <div class="row">
      <input id=${i === 0 ? id : nothing} type="text" list=${values} .value=${live(item === null ? "" : String(item))} ?readonly=${readOnly} @input=${edit(i)} />
      ${readOnly ? nothing : html`<button class="icon" title="Remove" @click=${() => change(items.filter((_, j) => j !== i))}>×</button>`}
    </div>
  `;
  return html`
    <div class="list">
      ${items.map(row)}
      ${readOnly ? nothing : html`<button class="small" @click=${() => change([...items, ""])}>Add</button>`}
      ${valuesList(`${id}-values`, suggestions)}
    </div>
  `;
}

function mapInput(obj: Record<string, unknown>, set: Setter, readOnly: boolean, path: Path): TemplateResult {
  const setKey = (k: string) => (v: unknown) => {
    obj[k] = v;
    set(obj);
  };
  return html`<fieldset class="group">${Object.keys(obj).map((k) => field(k, k, obj[k], setKey(k), readOnly, [...path, k], null))}</fieldset>`;
}

function objectsInput(key: string, items: Record<string, unknown>[], set: Setter, readOnly: boolean, path: Path): TemplateResult {
  const change = () => {
    set(items);
    pageView();
  };
  const remove = (i: number) => () => {
    items.splice(i, 1);
    change();
  };
  // A new item has the keys of the first, their values empty.
  const add = () => {
    const blank = (v: unknown) => (typeof v === "number" ? 0 : typeof v === "boolean" ? false : Array.isArray(v) ? [] : "");
    items.push(Object.fromEntries(Object.entries(items[0] ?? {}).map(([k, v]) => [k, blank(v)])));
    change();
  };
  const setKey = (item: Record<string, unknown>, k: string) => (v: unknown) => {
    item[k] = v;
    set(items);
  };
  const itemSet = (item: Record<string, unknown>, i: number) => html`
    <fieldset class="group item">
      <legend>${key} ${i + 1}${readOnly ? nothing : html`<button class="icon" title="Remove" @click=${remove(i)}>×</button>`}</legend>
      ${Object.keys(item).map((k) => field(k, k, item[k], setKey(item, k), readOnly, [...path, i, k], null))}
    </fieldset>
  `;
  return html`
    <div class="group objects">
      ${items.map(itemSet)}
      ${readOnly ? nothing : html`<button class="small" @click=${add}>Add ${key}</button>`}
    </div>
  `;
}

function imageInput(value: string, set: Setter, readOnly: boolean, id: string): TemplateResult {
  const names = imageChoices();
  return html`
    <span class="row">
      <select id=${id} ?disabled=${readOnly} @change=${(ev: Event) => set(valueOf(ev))}>
        <option value="">—</option>
        ${optionsOf(value && !names.includes(value) ? [value, ...names] : names, value)}
      </select>
      <small class="muted">Upload files below to add choices.</small>
    </span>
  `;
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
