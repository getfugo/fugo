// Moving, translations and deleting; saving, publishing and discarding drafts. New pages and
// folders are in create.ts.

import { create, join } from "../codec";
import { api, ApiError, messageOf } from "./api";
import { keepNewFolders, newFolderDocs, pathFor } from "./create";
import { type Change, type EntryFile } from "./data";
import { toast } from "./dom";
import { movedPath } from "./folders";
import { problems } from "./hints";
import { docText, makeDoc, openPage, pageView } from "./pages";
import { route } from "./route";
import { canEdit, current, loadDrafts, pending, sectionFor, site } from "./state";

/**
 * Moves the open page into `folder` (its section's own, or one below it), with all its files.
 * Each language's file keeps the URL it had in `aliases`, so that old links still work.
 */
export async function movePage(folder: string): Promise<void> {
  const p = current();
  const { entry } = p;
  const name = entry.key.split("/").pop() ?? entry.key;
  const key = folder ? `${folder}/${name}` : name;
  if (key === entry.key) return;
  if (pending.has(key) || site.entries.some((e) => e.key === key)) {
    toast(`A page ${key} exists already`, "error");
    return;
  }
  if (p.uploads.length > 0 || p.deletes.size > 0) {
    toast("Save the page's files first", "error");
    return;
  }
  const changes: Change[] = [];
  const files: EntryFile[] = [];
  for (const [lang, doc] of p.docs) {
    const path = movedPath(doc.path, entry.key, key);
    const moved = makeDoc(path, docText(doc), null, true);
    const url = entry.files.find((f) => f.lang === lang)?.url;
    if (url && moved.data) {
      const aliases = Array.isArray(moved.data.aliases) ? moved.data.aliases.map(String) : [];
      if (!aliases.includes(url)) moved.data.aliases = [...aliases, url];
    }
    if (!doc.isNew) changes.push({ path: doc.path, delete: true, base: doc.sha });
    changes.push({ path, content: docText(moved), encoding: "utf-8", base: null });
    files.push({ lang, path, format: moved.parts.format, title: String(moved.data?.title ?? "") });
  }
  const resources = entry.resources.map((r) => movedPath(r, entry.key, key));
  entry.resources.forEach((r, i) => changes.push({ path: r, delete: true }, { path: resources[i], from: r, base: null }));
  for (const d of newFolderDocs(key)) changes.push({ path: d.path, content: docText(d), encoding: "utf-8", base: null });
  if (!changes.every((c) => canEdit(c.path))) {
    toast("You may not move every file of this page", "error");
    return;
  }
  if (!confirm(`Move ${entry.title} to ${folder || "the top"}? Its URL changes; the old one keeps working.`)) return;
  try {
    const title = p.docs.get(site.default_language)?.data?.title ?? entry.title;
    // The save is the page's at its new key, in the draft it was opened from (in review).
    const res = await api<{ draft: string | null }>("POST", "save", null, { entry: key, title: String(title ?? ""), changes, draft: p.draft?.id });
    toast(res.draft ? "Moved, as a draft" : "Moved: the site updates after its next build", "ok");
    const old = site.entries.indexOf(entry);
    if (!res.draft && old >= 0) site.entries.splice(old, 1);
    const moved = { ...entry, key, files, resources, isNew: undefined };
    keepNewFolders(moved, res.draft ? key : null);
    site.entries.push(moved);
    await loadDrafts();
    await openPage(key);
  } catch (e) {
    const stale = e instanceof ApiError ? e.data.stale : undefined;
    toast(Array.isArray(stale) ? `${messageOf(e)}: ${stale.join(", ")}` : messageOf(e), "error");
  }
}

export function addTranslation(lang: string): void {
  const p = current();
  const section = sectionFor(p.entry.section);
  const from = p.docs.get(site.default_language) ?? [...p.docs.values()][0];
  const first = p.entry.files[0];
  let path: string;
  if (first && site.languages.some((l) => first.path.includes(`.${l.key}.`))) {
    path = first.path.replace(new RegExp(`\\.${first.lang}\\.([^./]+)$`), `.${lang}.$1`);
  } else {
    path = pathFor(section, p.entry.key.replace(/\/_index$/, ""), lang, p.entry.bundle);
    if (p.entry.kind !== "page") path = path.replace(/\/index(\.[^/]+)$/, "/_index$1");
  }
  if (!canEdit(path)) {
    toast(`You may not create ${path}`, "error");
    return;
  }
  const text = from ? docText(from) : join(create(section.style.format, { title: p.entry.title }, ""));
  p.docs.set(lang, makeDoc(path, text, null, true));
  p.lang = lang;
  pageView();
}

export async function deletePage(): Promise<void> {
  const p = current();
  const paths = [...p.docs.values()]
    .filter((d) => !d.isNew)
    .map((d) => d.path)
    .concat(p.entry.resources);
  if (!paths.every(canEdit)) {
    toast("You may not delete every file of this page", "error");
    return;
  }
  if (!confirm(`Delete ${p.entry.title} (${paths.length} files)?`)) return;
  const sha = new Map([...p.docs.values()].map((d) => [d.path, d.sha]));
  await send(paths.map((f): Change => (sha.has(f) ? { path: f, delete: true, base: sha.get(f) } : { path: f, delete: true })));
}

// ── Saving, publishing ─────────────────────────────────────────────────────────────────────────

export async function save(): Promise<void> {
  const p = current();
  const wrong = problems();
  if (wrong.length) {
    toast(`Not saved: ${wrong.join("; ")}`, "error");
    return;
  }
  const changes: Change[] = [];
  // A page in a new folder saves the folder's page (and those of new folders above it) too.
  for (const doc of [...newFolderDocs(p.entry.key), ...p.docs.values()]) {
    if (!canEdit(doc.path)) continue;
    let text: string;
    try {
      text = docText(doc);
    } catch (e) {
      toast(`${doc.path}: ${messageOf(e)}`, "error");
      return;
    }
    if (!doc.isNew && text === doc.initial) continue;
    changes.push({ path: doc.path, content: text, encoding: "utf-8", base: doc.sha });
  }
  for (const u of p.uploads) changes.push({ path: u.path, content: u.content, encoding: "base64" });
  for (const f of p.deletes) changes.push({ path: f, delete: true });
  if (changes.length === 0) {
    toast("Nothing changed");
    return;
  }
  await send(changes);
}

async function send(changes: Change[]): Promise<void> {
  const p = current();
  const title = p.docs.get(site.default_language)?.data?.title ?? p.entry.title;
  // A new folder's page saved with a page is in that page's draft: its changes go there too.
  const entry = p.draft?.entry || p.entry.key;
  try {
    // The files were read from the page's draft, if it has one: the save goes there too.
    const res = await api<{ draft: string | null }>("POST", "save", null, { entry, title: String(title ?? ""), changes, draft: p.draft?.id });
    toast(res.draft ? "Saved as a draft" : "Saved: the site updates after its next build", "ok");
    pending.delete(p.entry.key);
    keepNewFolders(p.entry, res.draft ? entry : null);
    // A file keeps the address its page has on the site (the preview and the text's images use it).
    const urlOf = (lang: string, path: string) => p.entry.files.find((f) => f.lang === lang && f.path === path)?.url;
    const files = [...p.docs.entries()].map(([lang, d]): EntryFile => ({ lang, path: d.path, format: d.parts.format, title: String(d.data?.title ?? ""), url: urlOf(lang, d.path) }));
    const known = site.entries.find((e) => e.key === p.entry.key);
    if (!known) {
      site.entries.push({ ...p.entry, isNew: undefined, files, resources: p.uploads.map((u) => u.path) });
    } else {
      known.files = files;
      known.resources = [...new Set([...known.resources, ...p.uploads.map((u) => u.path)])].filter((f) => !p.deletes.has(f));
    }
    await loadDrafts();
    await openPage(p.entry.key);
  } catch (e) {
    const stale = e instanceof ApiError ? e.data.stale : undefined;
    toast(Array.isArray(stale) ? `${messageOf(e)}: ${stale.join(", ")}` : messageOf(e), "error");
  }
}

export async function publish(id: string): Promise<void> {
  if (!confirm("Publish this draft to the site?")) return;
  try {
    const res = await api<{ published: boolean; kept?: boolean }>("POST", "publish", null, { id });
    toast(
      res.kept
        ? "Published; changes saved during publishing stay in the draft"
        : res.published === false
          ? "The draft had no changes; it is gone"
          : "Published: the site updates after its next build",
      "ok",
    );
    await loadDrafts();
    route();
  } catch (e) {
    const conflicts = e instanceof ApiError ? e.data.conflicts : undefined;
    toast(Array.isArray(conflicts) ? `${messageOf(e)}: ${conflicts.join(", ")}` : messageOf(e), "error");
  }
}

export async function discard(id: string): Promise<void> {
  if (!confirm("Discard this draft? Its changes are lost.")) return;
  try {
    await api("POST", "discard", null, { id });
    toast("Draft discarded", "ok");
    await loadDrafts();
    route();
  } catch (e) {
    toast(messageOf(e), "error");
  }
}
