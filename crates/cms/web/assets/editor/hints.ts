// The fields of the keys (`site.fields`, by path: `nutrition.fat` is `fat` in the table
// `nutrition`, and the keys of a list's tables are below the list's key): their settings, the keys
// a table can add, the values keys start with, the values one for every language, the checks
// before saving and the summaries of closed tables.

import { clone, type FrontMatter } from "../codec";
import { type FieldHint } from "./data";
import { canEdit, current, langName, site } from "./state";

/** Where a field is: its key, and the keys and indexes of the fields it is in. */
export type Path = (string | number)[];

/** The index's key of a field: the keys of its path, lower-cased, joined by dots. */
export const pathKey = (path: Path) =>
  path
    .filter((p): p is string => typeof p === "string")
    .join(".")
    .toLowerCase();

export function hintOf(path: Path | string): FieldHint {
  return site.fields?.[typeof path === "string" ? path.toLowerCase() : pathKey(path)] ?? {};
}

/** The keys the content or the settings have in the tables at `key` (as pages write them). */
export function childrenOf(key: string): { name: string; kind: string }[] {
  const prefix = `${key}.`;
  return Object.entries(site.fields ?? {})
    .filter(([k, h]) => k.startsWith(prefix) && !k.slice(prefix.length).includes(".") && h.widget !== "hidden")
    .map(([k, h]) => ({ name: h.name ?? k.slice(prefix.length), kind: h.kind ?? "string" }));
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

/** The value the key at `key` starts with: its setting's `default`, else `fallback` (the value
 * every page of the section gives it), else an empty value of its kind. */
export function startOf(key: string, kind: string, fallback?: unknown): unknown {
  const d = hintOf(key).default;
  return clone(d !== undefined ? d : fallback !== undefined ? fallback : emptyOf(kind));
}

/** Writes the value of the front matter key `key`, one for every language, to the open page's
 * other language files that the person may change (`undefined` removes it). */
export function share(from: FrontMatter, key: string, value: unknown): void {
  for (const doc of current().docs.values()) {
    if (!doc.data || doc.data === from || doc.raw || !canEdit(doc.path)) continue;
    const name = Object.keys(doc.data).find((k) => k.toLowerCase() === key.toLowerCase()) ?? key;
    if (value === undefined) delete doc.data[name];
    else doc.data[name] = clone(value);
  }
}

const isTable = (x: unknown): x is Record<string, unknown> => x !== null && typeof x === "object" && !Array.isArray(x) && !(x instanceof Date);
export const isEmpty = (v: unknown) => v === undefined || v === null || v === "" || (Array.isArray(v) && v.length === 0);

/** The values at the keys `keys` below `node`, through tables and the tables of lists; a key a
 * table lacks counts as `undefined` when it is the last, and as no value further up. */
function valuesAt(node: unknown, keys: string[]): unknown[] {
  if (keys.length === 0) return [node];
  if (Array.isArray(node)) return node.flatMap((item) => (isTable(item) ? valuesAt(item, keys) : []));
  if (!isTable(node)) return [];
  const name = Object.keys(node).find((k) => k.toLowerCase() === keys[0]);
  if (name === undefined) return keys.length === 1 ? [undefined] : [];
  return valuesAt(node[name], keys.slice(1));
}

/** What keeps the open page from being saved: values the settings require, and numbers below
 * `min` or above `max`. A required key of the front matter counts on a page of a section whose
 * pages use it, or that has it. */
export function problems(): string[] {
  const p = current();
  const section = site.sections.find((s) => s.key === p.entry.section);
  const out: string[] = [];
  for (const [lang, doc] of p.docs) {
    if (!doc.data || doc.raw || !canEdit(doc.path)) continue;
    const where = p.docs.size > 1 ? `${langName(lang)}: ` : "";
    for (const [key, h] of Object.entries(site.fields ?? {})) {
      if (!h.required && h.min === undefined && h.max === undefined) continue;
      const keys = key.split(".");
      const has = Object.keys(doc.data).some((k) => k.toLowerCase() === keys[0]);
      if (keys.length === 1 && !has && !section?.keys.some((k) => k.key.toLowerCase() === key)) continue;
      const label = h.label ?? key;
      for (const v of valuesAt(doc.data, keys)) {
        if (h.required && isEmpty(v)) out.push(`${where}${label} needs a value`);
        if (typeof v !== "number") continue;
        if (h.min !== undefined && v < h.min) out.push(`${where}${label} is below ${h.min}`);
        if (h.max !== undefined && v > h.max) out.push(`${where}${label} is above ${h.max}`);
      }
    }
  }
  return [...new Set(out)];
}

/** The summary of a closed table: its field's `summary` with its values in place of `{key}`
 * (`{fat.total}` for a key of a table in it), else its first three values. */
export function summaryOf(hint: FieldHint, table: Record<string, unknown>): string {
  const shown = (v: unknown) => (v === undefined || v === null ? "" : isTable(v) || Array.isArray(v) ? "…" : String(v));
  if (hint.summary) return hint.summary.replace(/\{([^{}]+)\}/g, (_, k: string) => shown(valuesAt(table, k.toLowerCase().split("."))[0]));
  return Object.values(table)
    .filter((v) => !isEmpty(v) && !isTable(v) && !Array.isArray(v))
    .slice(0, 3)
    .map(String)
    .join(" · ");
}
