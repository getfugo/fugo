// The folders below a section that a new page can go in.

import { type Entry, type Section } from "./data";

/** A folder for new pages: its key (its path below the content directory), title and depth. */
export interface Folder {
  key: string;
  title: string;
  depth: number;
}

/**
 * The folders below `section` that have an index page (`<dir>/_index`), in key order, each with
 * its depth below the section (1 for a folder directly in it). None for the root section, whose
 * folders are the other sections.
 */
export function foldersOf(section: Pick<Section, "key">, entries: Pick<Entry, "key" | "kind" | "title">[]): Folder[] {
  if (!section.key) return [];
  const prefix = `${section.key}/`;
  const depth0 = section.key.split("/").length;
  return entries
    .filter((e) => e.kind === "section" && e.key.startsWith(prefix) && e.key.endsWith("/_index"))
    .map((e) => {
      const key = e.key.slice(0, -"/_index".length);
      return { key, title: e.title, depth: key.split("/").length - depth0 };
    })
    .filter((f) => f.depth > 0)
    .sort((a, b) => a.key.localeCompare(b.key));
}
