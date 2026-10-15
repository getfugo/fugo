// The inputs of the form for text, dates, terms, several options and lists, the menu of the keys
// a folder's page may add, and the value offered under an empty field (images: `images.ts`).

import { html, nothing, type TemplateResult } from "lit-html";
import { live } from "lit-html/directives/live.js";
import { isTomlDate, tomlDate } from "../codec";
import { type Taxonomy } from "./data";
import { valueOf } from "./dom";
import { type Setter } from "./form";
import { isEmpty } from "./hints";
import { icon } from "./icons";
import { type MdCheckbox, type MdMenu, type MdOutlinedTextField } from "./material";
import { pageView } from "./pages";

/** The values pages share for a key, as a `<datalist>` for inputs to suggest them (`list=`). */
export const valuesList = (id: string, values: string[] | undefined) =>
  values?.length ? html`<datalist id=${id}>${values.map((v) => html`<option value=${v}></option>`)}</datalist>` : nothing;

/** The options of a select of `values` (`""` is none, shown as —), `chosen` selected (also when
 * another was chosen before a redraw). */
export const optionsOf = (values: string[], chosen: string) =>
  values.map((v) => html`<md-select-option value=${v} .selected=${live(v === chosen)}><div slot="headline">${v || "—"}</div></md-select-option>`);

/**
 * A line of text. With values to suggest (the `<datalist>` `list`), a plain input, which the
 * Material text field cannot be: its input is in its shadow tree, out of reach of a list's id.
 */
export function textInput(id: string | undefined, value: string, input: (ev: Event) => void, readOnly: boolean, list?: string, required = false): TemplateResult {
  return list
    ? html`<input id=${id ?? nothing} class="text" type="text" list=${list} ?required=${required} .value=${live(value)} ?readonly=${readOnly} @input=${input} />`
    : html`<md-outlined-text-field id=${id ?? nothing} ?required=${required} .value=${live(value)} ?readonly=${readOnly} @input=${input}></md-outlined-text-field>`;
}

/** A button that opens a menu of `keys` (their labels shown); `add` gets the one chosen. */
export function addMenu(label: string, keys: { key: string; label: string }[], add: (key: string) => void): TemplateResult {
  const toggle = (ev: Event) => {
    const menu = (ev.currentTarget as Element).nextElementSibling as MdMenu;
    menu.anchorElement = ev.currentTarget as HTMLElement;
    menu.open = !menu.open;
  };
  return html`
    <div class="add-field">
      <md-text-button @click=${toggle}>${icon("add", "icon")}${label}</md-text-button>
      <md-menu positioning="popover">
        ${keys.map((k) => html`<md-menu-item @click=${() => add(k.key)}><div slot="headline">${k.label}</div></md-menu-item>`)}
      </md-menu>
    </div>
  `;
}

/** Under an empty field: a value for it, `from` where it comes (another language, its default),
 * and the button `label` that `put`s it there. */
export function offer(from: string, value: unknown, label: string, put: () => void): TemplateResult {
  const shown = typeof value === "string" || typeof value === "number" || typeof value === "boolean" || isTomlDate(value) ? String(value) : "";
  return html`
    <div class="offer">
      <span class="muted" title=${shown}>${from}${shown ? `: ${shown}` : ""}</span>
      <md-text-button @click=${put}>${label}</md-text-button>
    </div>
  `;
}

export function dateInput(value: unknown, set: Setter, readOnly: boolean, id: string): TemplateResult {
  const toml = isTomlDate(value);
  const shown = toml ? value.toISOString() : typeof value === "string" ? value : "";
  const input = (ev: Event) => {
    const el = ev.target as MdOutlinedTextField;
    if (!toml) return set(el.value);
    const d = tomlDate(el.value);
    el.error = !d;
    if (d) set(d);
  };
  // Types the time into the field, as if by hand.
  const now = (ev: Event) => {
    const el = (ev.currentTarget as HTMLElement).previousElementSibling as MdOutlinedTextField;
    el.value = new Date().toISOString();
    el.dispatchEvent(new Event("input", { bubbles: true }));
  };
  return html`
    <span class="row">
      <md-outlined-text-field id=${id} class="date" placeholder="2026-01-31T12:00:00Z" .value=${live(shown)} ?readonly=${readOnly} @input=${input}></md-outlined-text-field>
      <md-text-button ?disabled=${readOnly} @click=${now}>${icon("schedule", "icon")}Now</md-text-button>
    </span>
  `;
}

export function termsInput(taxonomy: Taxonomy, value: unknown, set: Setter, readOnly: boolean, id: string): TemplateResult {
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
  // The chip would take itself out of the page; the form draws it again without it.
  const remove = (i: number) => (ev: Event) => {
    ev.preventDefault();
    store(items.filter((_, j) => j !== i));
  };
  const chip = (t: string, i: number) => html`<md-input-chip class="chip" label=${t} remove-only ?disabled=${readOnly} @remove=${remove(i)}></md-input-chip>`;
  return html`
    <span class="terms">
      ${items.length ? html`<md-chip-set>${items.map(chip)}</md-chip-set>` : nothing}
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
export function choicesInput(options: string[], value: unknown, set: Setter, readOnly: boolean, id: string): TemplateResult {
  let chosen = Array.isArray(value) ? value.map(String) : isEmpty(value) ? [] : [String(value)];
  const all = [...options, ...chosen.filter((c) => !options.includes(c))];
  const toggle = (option: string) => (ev: Event) => {
    const on = (ev.target as MdCheckbox).checked;
    chosen = on ? [...chosen, option] : chosen.filter((c) => c !== option);
    set(all.filter((o) => chosen.includes(o)));
  };
  return html`
    <span class="choices">
      ${all.map(
        (o, i) => html`
          <label>
            <md-checkbox id=${i === 0 ? id : nothing} .checked=${live(chosen.includes(o))} ?disabled=${readOnly} @change=${toggle(o)}></md-checkbox>
            ${o}
          </label>
        `,
      )}
    </span>
  `;
}

export function listInput(value: unknown[], set: Setter, readOnly: boolean, id: string, suggestions?: string[]): TemplateResult {
  const items = [...value];
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
  const values = suggestions?.length ? `${id}-values` : undefined;
  const row = (item: unknown, i: number) => html`
    <div class="row">
      ${textInput(i === 0 ? id : undefined, item === null ? "" : String(item), edit(i), readOnly, values)}
      ${readOnly ? nothing : html`<md-icon-button class="remove" title="Remove" aria-label="Remove" @click=${() => change(items.filter((_, j) => j !== i))}>${icon("close")}</md-icon-button>`}
    </div>
  `;
  return html`
    <div class="list">
      ${items.map(row)}
      ${readOnly ? nothing : html`<md-text-button @click=${() => change([...items, ""])}>${icon("add", "icon")}Add</md-text-button>`}
      ${valuesList(`${id}-values`, suggestions)}
    </div>
  `;
}
