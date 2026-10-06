// The inputs of the form for dates, terms, several options, lists and images.

import { html, nothing, type TemplateResult } from "lit-html";
import { live } from "lit-html/directives/live.js";
import { isTomlDate, tomlDate } from "../codec";
import { type Taxonomy } from "./data";
import { valueOf } from "./dom";
import { bundleDir } from "./files";
import { type Setter } from "./form";
import { isEmpty } from "./hints";
import { pageView } from "./pages";
import { current, site } from "./state";

/** The values pages share for a key, as a `<datalist>` for inputs to suggest them (`list=`). */
export const valuesList = (id: string, values: string[] | undefined) =>
  values?.length ? html`<datalist id=${id}>${values.map((v) => html`<option value=${v}></option>`)}</datalist>` : nothing;

/** The `<option>`s of `values`, `chosen` selected (also when another was chosen before a redraw). */
export const optionsOf = (values: string[], chosen: string) => values.map((v) => html`<option value=${v} .selected=${live(v === chosen)}>${v}</option>`);

export function dateInput(value: unknown, set: Setter, readOnly: boolean, id: string): TemplateResult {
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
export function choicesInput(options: string[], value: unknown, set: Setter, readOnly: boolean, id: string): TemplateResult {
  let chosen = Array.isArray(value) ? value.map(String) : isEmpty(value) ? [] : [String(value)];
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

export function listInput(value: unknown[], set: Setter, readOnly: boolean, id: string, suggestions?: string[]): TemplateResult {
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

export function imageInput(value: string, set: Setter, readOnly: boolean, id: string): TemplateResult {
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
