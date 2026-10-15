// Tables in the form: the keys of a table and of each table of a list, the keys other pages have
// in them (empty, where a table lacks them), and tables shown closed with a summary of their
// values.

import { html, nothing, type TemplateResult } from "lit-html";
import { clone } from "../codec";
import { field, type Setter } from "./form";
import { childrenOf, hintOf, isEmpty, type Path, pathKey, startOf, summaryOf } from "./hints";
import { icon } from "./icons";
import { offer } from "./inputs";
import { pageView } from "./pages";

type Table = Record<string, unknown>;

/** The fields of the table at `path`, then those of the keys other pages have in it that it
 * lacks, empty: a key goes into the table once it has a value. */
function tableFields(table: Table, set: Setter, readOnly: boolean, path: Path): TemplateResult {
  const key = pathKey(path);
  const has = (name: string) => Object.keys(table).some((k) => k.toLowerCase() === name.toLowerCase());
  const setKey = (k: string) => (v: unknown) => {
    if (!has(k) && isEmpty(v)) return;
    table[k] = v;
    set(table);
  };
  const remove = (k: string) => () => {
    delete table[k];
    set(table);
    pageView();
  };
  // Under a key the table lacks: its setting's default, and a button that puts it there.
  const below = (name: string) => {
    const d = hintOf(`${key}.${name}`).default;
    if (readOnly || d === undefined) return nothing;
    return offer("Default", d, "Use", () => {
      table[name] = clone(d);
      set(table);
      pageView();
    });
  };
  const shown = Object.keys(table).filter((k) => hintOf([...path, k]).widget !== "hidden");
  const lacks = childrenOf(key).filter((c) => !has(c.name));
  return html`
    ${shown.map((k) => field(k, hintOf([...path, k]).label ?? k, table[k], setKey(k), readOnly, [...path, k], remove(k)))}
    ${lacks.map((c) => field(c.name, hintOf(`${key}.${c.name}`).label ?? c.name, undefined, setKey(c.name), readOnly, [...path, c.name], null, below(c.name)))}
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
  const removeButton = (i: number) =>
    readOnly ? nothing : html`<md-icon-button class="remove" title="Remove" aria-label="Remove" @click=${remove(i)}>${icon("close")}</md-icon-button>`;
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
      ${readOnly ? nothing : html`<md-text-button @click=${add}>${icon("add", "icon")}Add ${label}</md-text-button>`}
    </div>
  `;
}
