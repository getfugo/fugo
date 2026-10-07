// New pages and folders, in any folder at any depth: where their files go, who may make them, and
// the pages of new folders, which are saved with the first page saved in them.

import { clone, create, type FrontMatter, join } from "../codec";
import { type Doc, type Entry, type Section } from "./data";
import { toast } from "./dom";
import { allFolders, folderHref, folderTitle, joinDir, parentDir, placeOf } from "./folders";
import { taxonomies } from "./form";
import { emptyOf, hintOf, startOf } from "./hints";
import { makeDoc } from "./pages";
import { allEntries, canEdit, pending, sectionFor, site } from "./state";

const slugify = (s: string) =>
  s
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);

/** The path of a page's file in `lang`, written like the section's pages. */
export function pathFor(section: Section, key: string, lang: string, bundle: boolean): string {
  const style = section.style;
  const langDir = site.languages.find((l) => l.key === lang)?.content_dir;
  const root = langDir ?? site.content_dir ?? "content";
  const suffix = style.lang_suffix || (!langDir && lang !== site.default_language) ? `.${lang}` : "";
  return bundle ? `${root}/${key}/index${suffix}.${style.ext}` : `${root}/${key}${suffix}.${style.ext}`;
}

/** The path of the page of the folder `dir` (`<dir>/_index.md`) in the default language. */
function folderPath(dir: string): string {
  return pathFor(sectionFor(dir), dir, site.default_language, true).replace(/\/index(\.[^/]+)$/, "/_index$1");
}

/** The path of the page `name` of the folder `dir` in the default language. */
function pagePath(dir: string, name: string): string {
  const section = sectionFor(dir);
  return pathFor(section, joinDir(dir, name), site.default_language, section.style.bundle);
}

/** The signed-in person may make a page, or a folder, in the folder `dir`. */
export const mayAddPage = (dir: string) => canEdit(pagePath(dir, "new-page"));
export const mayAddFolder = (dir: string) => canEdit(folderPath(joinDir(dir, "new-folder")));

/** Asks for the title of a new page in the folder `dir`, and makes it. */
export function askNewPage(dir: string): void {
  const title = prompt(`Title of the new page in ${folderTitle(dir, allEntries())}:`)?.trim();
  if (title) newPage(dir, title);
}

/** Asks for the title of a new folder in the folder `dir` (at the root: a section), and makes it. */
export function askNewFolder(dir: string): void {
  const title = prompt(dir ? `Title of the new folder in ${folderTitle(dir, allEntries())}:` : "Title of the new section:")?.trim();
  if (title) newFolder(dir, title);
}

/** A new page titled `title` in the folder `dir`, written like its section's pages. */
export function newPage(dir: string, title: string): void {
  const section = sectionFor(dir);
  const key = joinDir(dir, slugify(title) || `page-${Date.now()}`);
  if (pending.has(key)) {
    location.hash = `#/e/${encodeURIComponent(key)}`;
    return;
  }
  if (site.entries.some((e) => e.key === key) || allFolders(allEntries()).includes(key)) {
    toast(`${key} exists already`, "error");
    return;
  }
  const lang = site.default_language;
  const path = pagePath(dir, key.slice(key.lastIndexOf("/") + 1));
  if (!canEdit(path)) {
    toast(`You may not create ${path}`, "error");
    return;
  }
  const data: FrontMatter = { title, date: new Date().toISOString() };
  const terms = taxonomies();
  const has = (key: string) => Object.keys(data).some((x) => x.toLowerCase() === key.toLowerCase());
  for (const k of section.keys) {
    if (has(k.key)) continue;
    // A key's default, or the value every page of the section gives it; taxonomy keys start as
    // empty lists (an empty string could name a term).
    if (hintOf(k.key).default !== undefined || k.default !== undefined) data[k.key] = startOf(k.key, k.kind, k.default);
    else if (terms.has(k.key.toLowerCase())) data[k.key] = [];
    else if (["string", "list", "boolean"].includes(k.kind)) data[k.key] = emptyOf(k.kind);
  }
  // Keys only the settings name, with a default.
  for (const [key, h] of Object.entries(site.fields ?? {})) {
    if (h.unused && h.default !== undefined && !key.includes(".") && !has(key)) data[h.name ?? key] = clone(h.default);
  }
  const doc = makeDoc(path, join(create(section.style.format, data, "")), null, true);
  pending.set(key, { key, section: section.key, kind: "page", bundle: section.style.bundle, title, resources: [], isNew: true, files: [{ lang, path, doc }] });
  location.hash = `#/e/${encodeURIComponent(key)}`;
}

/**
 * A new folder titled `title` in the folder `parent` (at the root: a new section), with its page
 * (`_index`), which holds its title and text. The view of the new folder opens, for its pages:
 * the folder's page is saved with the first of them, or on its own.
 */
export function newFolder(parent: string, title: string): void {
  const dir = joinDir(parent, slugify(title) || `folder-${Date.now()}`);
  const entries = allEntries();
  if (allFolders(entries).includes(dir)) {
    toast(`${dir} exists already`);
    location.hash = folderHref(dir);
    return;
  }
  if (entries.some((e) => e.key === dir)) {
    toast(`A page ${dir} exists already`, "error");
    return;
  }
  const lang = site.default_language;
  const path = folderPath(dir);
  if (!canEdit(path)) {
    toast(`You may not create ${path}`, "error");
    return;
  }
  const section = sectionFor(dir);
  const doc = makeDoc(path, join(create(section.style.format, { title }, "")), null, true);
  const key = `${dir}/_index`;
  pending.set(key, { key, section: section.key, kind: "section", bundle: false, title, resources: [], isNew: true, files: [{ lang, path, doc }] });
  location.hash = folderHref(dir);
}

/** The new folders above the page `key` that are not saved yet, the outermost first. */
function newFoldersAbove(key: string): Entry[] {
  const out: Entry[] = [];
  for (let dir = placeOf(key); dir; dir = parentDir(dir)) {
    const e = pending.get(`${dir}/_index`);
    if (e) out.unshift(e);
  }
  return out;
}

/** The files of the new folders above the page `key`, which a save of the page writes too. */
export function newFolderDocs(key: string): Doc[] {
  return newFoldersAbove(key).flatMap((e) => e.files.flatMap((f) => (f.doc ? [f.doc] : [])));
}

/**
 * Once the page `entry` is saved (with `workflow = "review"`, in the draft of the page
 * `draftPage`): the new folders above it, saved with it, are in the index, their files in that
 * draft; and so is its section, when it is a new one.
 */
export function keepNewFolders(entry: Entry, draftPage: string | null): void {
  for (const e of newFoldersAbove(entry.key)) {
    pending.delete(e.key);
    const files = e.files.map((f) => ({ lang: f.lang, path: f.path, format: f.doc?.parts.format, title: String(f.doc?.data?.title ?? e.title) }));
    site.entries.push({ ...e, isNew: undefined, files, savedWith: draftPage ?? undefined });
  }
  if (!site.sections.some((s) => s.key === entry.section)) site.sections.push(sectionFor(entry.section));
}
