// The editor's state: the content index, the signed-in person, their drafts and the open page.

import { mayEdit } from "../common";
import { api } from "./api";
import { type Draft, type Entry, type Me, type Page, type Section, type Site } from "./data";
import { folderTitle } from "./folders";

export let site!: Site;
export let me!: Me;
export let drafts: Draft[] = [];
export let page: Page | null = null;
/** Pages made in the editor and not saved yet, by key. */
export const pending = new Map<string, Entry>();

/** Loads the content index and the signed-in person, then their drafts. */
export async function loadSite(): Promise<void> {
  site = await api<Site>("GET", "site");
  me = await api<Me>("GET", "me");
  await loadDrafts();
}

/** Opens a page (`null`: none). */
export function setPage(open: Page | null): void {
  page = open;
}

export async function loadDrafts(): Promise<void> {
  drafts = me.workflow === "review" ? (await api<{ drafts: Draft[] }>("GET", "drafts")).drafts : [];
}

export const draftOf = (key: string) => drafts.find((d) => d.entry === key) ?? null;
export const canEdit = (path: string) => mayEdit(me, me.edit, path);
export const langName = (key: string) => site.languages.find((l) => l.key === key)?.name ?? key;
/** The pages of the index, and those made in the editor and not saved yet. */
export const allEntries = (): Entry[] => [...site.entries, ...pending.values()];
/** The taxonomy whose terms the folder `dir` holds: that of its section (`tags`). */
export const taxonomyOf = (dir: string) => site.taxonomies.find((t) => t.plural === dir.split("/")[0]);

/** The section of folder `dir` (its first name; `""` at the root): the index's, or a new one,
 * whose pages are written like those of the largest section. */
export function sectionFor(dir: string): Section {
  const key = dir.split("/")[0];
  const known = site.sections.find((s) => s.key === key);
  if (known) return known;
  const model = site.sections.reduce<Section | undefined>((a, s) => (a && a.count >= s.count ? a : s), undefined);
  const style = model?.style ?? { bundle: false, lang_suffix: false, format: "yaml", ext: "md" };
  return { key, title: folderTitle(key, allEntries()), count: 0, folders: 0, style, keys: [] };
}

export function current(): Page {
  if (!page) throw new Error("no page is open");
  return page;
}
