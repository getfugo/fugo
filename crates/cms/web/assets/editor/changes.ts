// New pages, translations and deleting; saving, publishing and discarding drafts.

import { create, type FrontMatter, join } from "../codec";
import { api, ApiError, messageOf } from "./api";
import { type Change, type EntryFile, type Section } from "./data";
import { toast } from "./dom";
import { emptyOf, taxonomies } from "./form";
import { docText, makeDoc, openPage, pageView } from "./pages";
import { route } from "./route";
import { canEdit, current, loadDrafts, pending, site } from "./state";

const slugify = (s: string) =>
  s
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);

/** The path of a page's file in `lang`, written like the section's pages. */
function pathFor(section: Section, key: string, lang: string, bundle: boolean): string {
  const style = section.style;
  const langDir = site.languages.find((l) => l.key === lang)?.content_dir;
  const root = langDir ?? site.content_dir ?? "content";
  const suffix = style.lang_suffix || (!langDir && lang !== site.default_language) ? `.${lang}` : "";
  return bundle ? `${root}/${key}/index${suffix}.${style.ext}` : `${root}/${key}${suffix}.${style.ext}`;
}

/** Asks for the title of a new page in the section's own folder, and makes it. */
export function askNewPage(section: Section): void {
  const title = prompt(`Title of the new page in ${section.title}:`)?.trim();
  if (title) newPage(section, title);
}

/** A new page titled `title` in `folder`: the section's own, or one below it (`foldersOf`). */
export function newPage(section: Section, title: string, folder = section.key): void {
  const slug = slugify(title) || `page-${Date.now()}`;
  const key = folder ? `${folder}/${slug}` : slug;
  if (site.entries.some((e) => e.key === key)) {
    toast(`A page ${key} exists already`, "error");
    return;
  }
  const lang = site.default_language;
  const path = pathFor(section, key, lang, section.style.bundle);
  if (!canEdit(path)) {
    toast(`You may not create ${path}`, "error");
    return;
  }
  const data: FrontMatter = { title, date: new Date().toISOString() };
  const terms = taxonomies();
  for (const k of section.keys) {
    if (Object.keys(data).some((x) => x.toLowerCase() === k.key.toLowerCase())) continue;
    // Taxonomy keys start as empty lists (an empty string could name a term).
    if (terms.has(k.key.toLowerCase())) data[k.key] = [];
    else if (["string", "list", "boolean"].includes(k.kind)) data[k.key] = emptyOf(k.kind);
  }
  const doc = makeDoc(path, join(create(section.style.format, data, "")), null, true);
  pending.set(key, { key, section: section.key, kind: "page", bundle: section.style.bundle, title, resources: [], isNew: true, files: [{ lang, path, doc }] });
  location.hash = `#/e/${encodeURIComponent(key)}`;
}

export function addTranslation(lang: string): void {
  const p = current();
  const section = site.sections.find((s) => s.key === p.entry.section) ?? site.sections[0];
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
  const changes: Change[] = [];
  for (const doc of p.docs.values()) {
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
  try {
    const res = await api<{ draft: string | null }>("POST", "save", null, { entry: p.entry.key, title: String(title ?? ""), changes });
    toast(res.draft ? "Saved as a draft" : "Saved: the site updates after its next build", "ok");
    pending.delete(p.entry.key);
    const files = [...p.docs.entries()].map(([lang, d]): EntryFile => ({ lang, path: d.path, format: d.parts.format, title: String(d.data?.title ?? "") }));
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
