// Content files in the editor: the front matter split off and decoded (YAML, TOML, JSON), and
// written back. An unchanged front matter is written back byte for byte; a changed YAML front
// matter keeps its comments and the layout of the keys that did not change.

import * as YAML from "yaml";
import { parse as parseToml, stringify as stringifyToml, TomlDate } from "smol-toml";

/** Decoded front matter. */
export type FrontMatter = Record<string, unknown>;

/** A content file in parts: joining them gives the file back. */
export interface Parts {
  format: "yaml" | "toml" | "json" | "none";
  bom: string;
  lead: string;
  open: string;
  close: string;
  front: string;
  body: string;
}

/** Splits a content file into its parts. */
export function split(text: string): Parts {
  const bom = text.startsWith("﻿") ? "﻿" : "";
  let src = text.slice(bom.length);
  const lead = /^(?:[ \t]*\r?\n)*/.exec(src)?.[0] ?? "";
  src = src.slice(lead.length);
  const open = /^(---|\+\+\+)[ \t]*(\r?\n)/.exec(src);
  if (open) {
    const delim = open[1] === "---" ? "---" : "\\+\\+\\+";
    const rest = src.slice(open[0].length);
    const close = new RegExp(`^${delim}[ \\t]*(?:\\r?\\n|$)`, "m").exec(rest);
    if (close) {
      return {
        format: open[1] === "---" ? "yaml" : "toml",
        bom,
        lead,
        open: open[0],
        close: close[0],
        front: rest.slice(0, close.index),
        body: rest.slice(close.index + close[0].length),
      };
    }
  }
  if (src.startsWith("{")) {
    const end = jsonEnd(src);
    if (end > 0) {
      return { format: "json", bom, lead, open: "", close: "", front: src.slice(0, end), body: src.slice(end) };
    }
  }
  return { format: "none", bom, lead: "", open: "", close: "", front: "", body: lead + src };
}

/** The file of `parts`. */
export function join(parts: Parts): string {
  return parts.bom + parts.lead + parts.open + parts.front + parts.close + parts.body;
}

/** The end of the JSON object at the start of `src` (after its `}`), or 0. */
function jsonEnd(src: string): number {
  let depth = 0;
  let inString = false;
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (inString) {
      if (c === "\\") i++;
      else if (c === '"') inString = false;
    } else if (c === '"') {
      inString = true;
    } else if (c === "{") {
      depth++;
    } else if (c === "}") {
      depth--;
      if (depth === 0) return i + 1;
    }
  }
  return 0;
}

/** The front matter of `parts` as a plain object (TOML dates stay `TomlDate`s).
 * @throws {Error} invalid front matter */
export function decode(parts: Parts): FrontMatter {
  let data: unknown;
  switch (parts.format) {
    case "yaml": {
      const doc = YAML.parseDocument(parts.front);
      if (doc.errors.length > 0) throw new Error(doc.errors[0].message);
      data = doc.toJS() ?? {};
      break;
    }
    case "toml":
      data = parseToml(parts.front);
      break;
    case "json":
      data = JSON.parse(parts.front);
      break;
    default:
      data = {};
  }
  if (data === null || typeof data !== "object" || Array.isArray(data)) {
    throw new Error("the front matter is not a map of keys");
  }
  return data as FrontMatter;
}

/** `parts` with front matter `data` (`original`: what `decode` returned; unchanged data keeps
 * the text). A file without front matter gets YAML when `data` has keys. */
export function encode(parts: Parts, original: FrontMatter, data: FrontMatter): Parts {
  if (equal(original, data)) return parts;
  const format = parts.format === "none" ? "yaml" : parts.format;
  const out: Parts = { ...parts, format };
  if (parts.format === "none") {
    if (Object.keys(data).length === 0) return parts;
    out.open = "---\n";
    out.close = "---\n";
    out.lead = "";
  }
  switch (format) {
    case "yaml": {
      const doc = parts.format === "yaml" ? YAML.parseDocument(parts.front) : null;
      if (doc && YAML.isMap(doc.contents)) {
        for (const key of Object.keys(original)) {
          if (!(key in data)) doc.delete(key);
        }
        for (const [key, value] of Object.entries(data)) {
          if (!equal(original[key], value)) doc.set(key, value);
        }
        out.front = keepUnchanged(parts.front, doc.toString({ lineWidth: 0 }), original, data);
      } else {
        out.front = YAML.stringify(data, { lineWidth: 0 });
      }
      break;
    }
    case "toml":
      out.front = toml(data);
      break;
    case "json":
      out.front = JSON.stringify(data, null, 2);
      if (!/^\r?\n/.test(out.body)) out.body = `\n${out.body}`;
      break;
  }
  return out;
}

/** A new content file of `format` with front matter `data` and `body`. */
export function create(format: string, data: FrontMatter, body: string): Parts {
  const parts: Parts = { format: "none", bom: "", lead: "", open: "", close: "", front: "", body };
  if (format === "toml") {
    return { ...parts, format: "toml", open: "+++\n", close: "+++\n", front: toml(data) };
  }
  if (format === "json") {
    return { ...parts, format: "json", front: JSON.stringify(data, null, 2), body: `\n${body}` };
  }
  return { ...parts, format: "yaml", open: "---\n", close: "---\n", front: YAML.stringify(data, { lineWidth: 0 }) };
}

/** `fresh` (the YAML library's text of the changed document) with the text of every top-level
 * key whose value did not change taken from `old`: the library re-renders some untouched values
 * (folded plain scalars), and an edit should not touch them. Falls back to `fresh` when the
 * result would not read back as `data`. */
function keepUnchanged(old: string, fresh: string, original: FrontMatter, data: FrontMatter): string {
  const a = topLevelBlocks(old);
  const b = topLevelBlocks(fresh);
  if (!a || !b) return fresh;
  let out = a.head;
  for (const key of b.order) {
    const same = a.blocks.has(key) && Object.hasOwn(original, key) && equal(original[key], data[key]);
    let block = (same ? a.blocks.get(key) : b.blocks.get(key)) ?? "";
    if (!block.endsWith("\n")) block += "\n";
    out += block;
  }
  try {
    return equal((YAML.parseDocument(out).toJS() ?? {}) as FrontMatter, data) ? out : fresh;
  } catch {
    return fresh;
  }
}

interface Blocks {
  head: string;
  blocks: Map<string, string>;
  order: string[];
}

/** The text of a YAML map's top-level keys: each key's lines up to the next key's line, and
 * what comes before the first key. Null for anything else (a flow map, a scalar). */
function topLevelBlocks(text: string): Blocks | null {
  const doc = YAML.parseDocument(text);
  if (doc.errors.length || !YAML.isMap(doc.contents) || doc.contents.flow) return null;
  const items = doc.contents.items;
  const lineStart = (i: number) => text.lastIndexOf("\n", i - 1) + 1;
  const starts: number[] = [];
  for (const pair of items) {
    if (!YAML.isScalar(pair.key) || !pair.key.range) return null;
    starts.push(lineStart(pair.key.range[0]));
  }
  const blocks = new Map<string, string>();
  const order: string[] = [];
  items.forEach((pair, i) => {
    const key = String((pair.key as YAML.Scalar).value);
    order.push(key);
    blocks.set(key, text.slice(starts[i], i + 1 < starts.length ? starts[i + 1] : text.length));
  });
  return { head: text.slice(0, starts[0] ?? text.length), blocks, order };
}

/** TOML text of `data`, ending in one line break. */
function toml(data: FrontMatter): string {
  const text = stringifyToml(data);
  return text.endsWith("\n") ? text : `${text}\n`;
}

/** Whether a value is a TOML date. */
export const isTomlDate = (v: unknown): v is TomlDate => v instanceof TomlDate;

/** A TOML date from text (`2024-01-02T03:04:05Z`, `2024-01-02`), or null when it is not one. */
export function tomlDate(text: string): TomlDate | null {
  try {
    const d = new TomlDate(text);
    return Number.isNaN(d.getTime()) ? null : d;
  } catch {
    return null;
  }
}

/** A deep copy (dates are kept as they are: they are not changed in place). */
export function clone<T>(v: T): T {
  if (Array.isArray(v)) return v.map(clone) as T;
  if (v instanceof Date) return v;
  if (v !== null && typeof v === "object") {
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, clone(x)])) as T;
  }
  return v;
}

/** Deep equality of decoded front matter values (key order counts). */
export function equal(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a instanceof Date || b instanceof Date) {
    return a instanceof Date && b instanceof Date && a.toISOString() === b.toISOString();
  }
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((x, i) => equal(x, b[i]));
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    const ra = a as Record<string, unknown>;
    const rb = b as Record<string, unknown>;
    return ka.length === kb.length && ka.every((k, i) => k === kb[i] && equal(ra[k], rb[k]));
  }
  return false;
}
