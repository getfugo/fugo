// Tables in the form: the keys of a table and of each table of a list, the keys other pages have
// in them (to add), and tables shown closed with a summary of their values.

import { html, nothing, type TemplateResult } from "lit-html";
import { field, type Setter } from "./form";
import { childrenOf, hintOf, type Path, pathKey, startOf, summaryOf } from "./hints";
import { pageView } from "./pages";

type Table = Record<string, unknown>;

/** The fields of the table at `path`, and a menu of the keys other pages have in it. */
function tableFields(table: Table, set: Setter, readOnly: boolean, path: Path): TemplateResult {
  const key = pathKey(path);
  const setKey = (k: string) => (v: unknown) => {
    table[k] = v;
    set(table);
  };
  const remove = (k: string) => () => {
    delete table[k];
    set(table);
    pageView();
  };
  const has = (name: string) => Object.keys(table).some((k) => k.toLowerCase() === name.toLowerCase());
  const missing = childrenOf(key).filter((c) => !has(c.name));
  const add = (ev: Event) => {
    const select = ev.target as HTMLSelectElement;
    const c = missing.find((x) => x.name === select.value);
    // Back to "Add…": the menu is drawn again without the key.
    select.value = "";
    if (!c) return;
    table[c.name] = startOf(`${key}.${c.name}`, c.kind);
    set(table);
    pageView();
  };
  const shown = Object.keys(table).filter((k) => hintOf([...path, k]).widget !== "hidden");
  return html`
    ${shown.map((k) => field(k, hintOf([...path, k]).label ?? k, table[k], setKey(k), readOnly, [...path, k], remove(k)))}
    ${!readOnly && missing.length
      ? html`
          <div class="add-field">
            <select @change=${add}>
              <option value="">Add…</option>
              ${missing.map((c) => html`<option value=${c.name}>${hintOf(`${key}.${c.name}`).label ?? c.name}</option>`)}
            </select>
          </div>
        `
      : nothing}
  `;
}

/** A table: its fields in a box, or closed with a summary when its field says `collapsed`. */
export function mapInput(table: Table, set: Setter, readOnly: boolean, path: Path): TemplateResult {
  const hint = hintOf(path);
  const fields = tableFields(table, set, readOnly, path);
  return hint.collapsed
    ? html`<details class="group"><summary>${summaryOf(hint, table) || "…"}</summary><div class="group-fields">${fields}</div></details>`
    : html`<fieldset class="group">${fields}</fieldset>`;
}

/** A list of tables: a box for each, or each closed with a summary when its field says
 * `collapsed`; a new one has the keys other pages have in them (else those of the first). */
export function objectsInput(key: string, items: Table[], set: Setter, readOnly: boolean, path: Path): TemplateResult {
  const hint = hintOf(path);
  const label = hint.label ?? key;
  const change = () => {
    set(items);
    pageView();
  };
  const remove = (i: number) => () => {
    items.splice(i, 1);
    change();
  };
  const add = () => {
    const children = childrenOf(pathKey(path));
    const blank = (v: unknown) => (typeof v === "number" ? 0 : typeof v === "boolean" ? false : Array.isArray(v) ? [] : "");
    items.push(
      children.length
        ? Object.fromEntries(children.map((c) => [c.name, startOf(`${pathKey(path)}.${c.name}`, c.kind)]))
        : Object.fromEntries(Object.entries(items[0] ?? {}).map(([k, v]) => [k, blank(v)])),
    );
    change();
  };
  const removeButton = (i: number) => (readOnly ? nothing : html`<button class="icon" title="Remove" @click=${remove(i)}>×</button>`);
  const item = (table: Table, i: number) => {
    const fields = tableFields(table, () => set(items), readOnly, [...path, i]);
    return hint.collapsed
      ? html`
          <details class="group item">
            <summary>${label} ${i + 1}: ${summaryOf(hint, table)}${removeButton(i)}</summary>
            <div class="group-fields">${fields}</div>
          </details>
        `
      : html`<fieldset class="group item"><legend>${label} ${i + 1}${removeButton(i)}</legend>${fields}</fieldset>`;
  };
  return html`
    <div class="group objects">
      ${items.map(item)}
      ${readOnly ? nothing : html`<button class="small" @click=${add}>Add ${label}</button>`}
    </div>
  `;
}
