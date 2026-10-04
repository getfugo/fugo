// The editor's state: the content index, the signed-in person, their drafts and the open page.

import { mayEdit } from "../common";
import { api } from "./api";
import { type Draft, type Entry, type Me, type Page, type Site } from "./data";

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

export function current(): Page {
  if (!page) throw new Error("no page is open");
  return page;
}
