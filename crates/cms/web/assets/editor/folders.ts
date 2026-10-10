// The folders of the content, at any depth: the directories that hold pages, with an index page
// (`<dir>/_index`) or without; and where the files of a moved page go.

import { type Entry, type Section } from "./data";

/** A folder for new pages: its key (its path below the content directory), title and depth. */
export interface Folder {
  key: string;
  title: string;
  depth: number;
}

/** The folder above `dir` (`""`, the root, above a top-level one). */
export const parentDir = (dir: string) => (dir.includes("/") ? dir.slice(0, dir.lastIndexOf("/")) : "");

/** `dir` and `name` as one path (`name` alone at the root). */
export const joinDir = (dir: string, name: string) => (dir ? `${dir}/${name}` : name);

/** The address of the view of folder `dir` (`#/s/` for the root). */
export const folderHref = (dir: string) => `#/s/${dir.split("/").map(encodeURIComponent).join("/")}`;

/** The folder whose own page `key` is (`a/b/_index` → `a/b`, the home page `_index` → `""`);
 * null for any other page. */
export function ownFolder(key: string): string | null {
  if (key === "_index") return "";
  return key.endsWith("/_index") ? key.slice(0, -"/_index".length) : null;
}

/** The folder a page is listed in: the one it is in, and for a folder's own page the folder
 * above; null for the home page, the root's own. */
export function placeOf(key: string): string | null {
  const own = ownFolder(key);
  return own === "" ? null : parentDir(own ?? key);
}

/** Every folder below the root that holds a page, or is one's own (`a/b/x` makes `a` and
 * `a/b`), sorted. */
export function allFolders(entries: Pick<Entry, "key">[]): string[] {
  const out = new Set<string>();
  for (const e of entries) {
    for (let dir = ownFolder(e.key) ?? parentDir(e.key); dir && !out.has(dir); dir = parentDir(dir)) out.add(dir);
  }
  return [...out].sort();
}

/** The folders that hold a page or another folder, besides their own page. */
export function filledFolders(entries: Pick<Entry, "key">[]): Set<string> {
  const out = new Set<string>();
  for (const e of entries) {
    const own = ownFolder(e.key);
    for (let dir = own === null ? parentDir(e.key) : own && parentDir(own); dir && !out.has(dir); dir = parentDir(dir)) out.add(dir);
  }
  return out;
}

/** `potato-chips` → `Potato chips`; the root is `Pages` (as the index names sections). */
function nameTitle(name: string): string {
  const words = name.replace(/[-_]/g, " ");
  return words ? words[0].toUpperCase() + words.slice(1) : "Pages";
}

/** The title of folder `dir`: its own page's, else made from its name. */
export function folderTitle(dir: string, entries: Pick<Entry, "key" | "title">[]): string {
  const own = dir ? entries.find((e) => e.key === `${dir}/_index`) : undefined;
  return own?.title || nameTitle(dir.slice(dir.lastIndexOf("/") + 1));
}

/**
 * The folders below `section`, in key order, each with its depth below the section (1 for a
 * folder directly in it). None for the root section, whose folders are the other sections.
 */
export function foldersOf(section: Pick<Section, "key">, entries: Pick<Entry, "key" | "kind" | "title">[]): Folder[] {
  if (!section.key) return [];
  const depth0 = section.key.split("/").length;
  return allFolders(entries)
    .filter((key) => key.startsWith(`${section.key}/`))
    .map((key) => ({ key, title: folderTitle(key, entries), depth: key.split("/").length - depth0 }))
    .sort((a, b) => a.key.localeCompare(b.key));
}

/**
 * The path of a file of the page at `oldKey` once the page is at `newKey`: its folder (a bundle,
 * `content/a/x/index.md` → `content/b/x/index.md`) or its name (`content/a/x.md` →
 * `content/b/x.md`) replaced.
 */
export function movedPath(path: string, oldKey: string, newKey: string): string {
  for (const end of ["/", "."]) {
    const at = path.indexOf(`/${oldKey}${end}`);
    if (at >= 0) return `${path.slice(0, at)}/${newKey}${path.slice(at + 1 + oldKey.length)}`;
  }
  throw new Error(`${path} is not a file of ${oldKey}`);
}
