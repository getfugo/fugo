// The view of a folder of the content, at any depth: the folders and pages in it, a filter that
// finds them anywhere below it, and new pages and folders in it. The root's folders are the
// sections.

import { html, nothing, type TemplateResult } from "lit-html";
import { askNewFolder, askNewPage, mayAddFolder, mayAddPage } from "./create";
import { type Entry } from "./data";
import { show, valueOf } from "./dom";
import { allFolders, filledFolders, folderHref, folderTitle, ownFolder, parentDir, placeOf } from "./folders";
import { allEntries, draftOf, taxonomyOf } from "./state";
import { holds, notFound } from "./views";

/** Links to the root and to each folder down to `dir`. */
export function crumbs(dir: string): TemplateResult {
  const entries = allEntries();
  const dirs = [""];
  if (dir) dir.split("/").forEach((name, i) => dirs.push(i ? `${dirs[i]}/${name}` : name));
  return html`${dirs.map((d, i) => html`${i ? " / " : ""}<a href=${folderHref(d)}>${folderTitle(d, entries)}</a>`)}`;
}

/**
 * The terms of a taxonomy with nothing in them but their own page (`tags/crisp`): they are
 * listed as that page, the term's, and not as empty folders. A term with terms below it (in a
 * hierarchical taxonomy) stays a folder.
 */
export function termFolders(entries: Entry[]): Set<string> {
  const filled = filledFolders(entries);
  const out = new Set<string>();
  for (const e of entries) {
    const dir = ownFolder(e.key);
    if (dir?.includes("/") && taxonomyOf(dir) && !filled.has(dir)) out.add(dir);
  }
  return out;
}

/** The folder whose list has the page `key`, where the page's crumbs end: the one it is in; for a
 * folder's own page, that folder, unless the page is a term's, listed in the folder above. */
export function listedIn(key: string): string {
  const own = ownFolder(key);
  return own === null || termFolders(allEntries()).has(own) ? (placeOf(key) ?? "") : own;
}

/** The filter of the folder shown. It stays while the view is redrawn, until another folder is
 * shown. */
const state = { dir: "", filter: "" };

export function folderView(dir: string): void {
  const entries = allEntries();
  const folders = allFolders(entries);
  if (dir && !folders.includes(dir)) return notFound();
  if (state.dir !== dir) Object.assign(state, { dir, filter: "" });
  const query = state.filter.trim().toLowerCase();
  // Without a filter, what is in the folder; with one, what matches anywhere below it.
  const listed = (key: string) => (query ? !dir || key.startsWith(`${dir}/`) : parentDir(key) === dir);
  const matches = (title: string, key: string) => !query || `${title} ${key}`.toLowerCase().includes(query);
  const byTitle = (a: { title: string }, b: { title: string }) => a.title.localeCompare(b.title);
  const terms = termFolders(entries);
  const subfolders = folders
    .filter((key) => listed(key) && !terms.has(key))
    .map((key) => ({ key, title: folderTitle(key, entries) }))
    .filter((f) => matches(f.title, f.key))
    .sort(byTitle);
  // A page's path: its key, or for a term's page, listed as a page, its folder.
  const pathOf = (e: Entry) => ownFolder(e.key) ?? e.key;
  const pages = entries
    .filter((e) => (ownFolder(e.key) === null || terms.has(pathOf(e))) && listed(pathOf(e)) && matches(e.title, pathOf(e)))
    .sort(byTitle);
  // Where a match of the filter is, below this folder.
  const where = (key: string) => parentDir(key).slice(dir ? dir.length + 1 : 0);
  const own = entries.find((e) => ownFolder(e.key) === dir);
  const title = folderTitle(dir, entries);
  show(html`
    ${dir ? html`<div class="crumbs">${crumbs(parentDir(dir))}</div>` : nothing}
    <div class="title-row">
      <h1>${title}</h1>
      ${own?.isNew ? html`<span class="badge pending">new</span>` : nothing}
      ${mayAddPage(dir) ? html`<button class="primary" @click=${() => askNewPage(dir)}>New page</button>` : nothing}
      ${mayAddFolder(dir) ? html`<button @click=${() => askNewFolder(dir)}>${dir ? "New folder" : "New section"}</button>` : nothing}
      ${own ? html`<a class="button" href="#/e/${encodeURIComponent(own.key)}">${dir ? "Edit folder page" : "Edit home page"}</a>` : nothing}
    </div>
    ${own?.isNew ? html`<p class="muted">A new folder: its page is saved with the first page you save in it, or on its own.</p>` : nothing}
    <input
      type="search"
      class="filter"
      placeholder="Filter by title or path, in this folder and below"
      .value=${state.filter}
      @input=${(ev: Event) => {
        state.filter = valueOf(ev);
        folderView(dir);
      }}
    />
    <table class="entries">
      <thead>
        <tr><th>Title</th><th>Languages</th><th></th></tr>
      </thead>
      <tbody>
        ${subfolders.map((f) => folderRow(f.key, f.title, where(f.key), entries))}
        ${pages.map((e) => pageRow(e, where(pathOf(e))))}
      </tbody>
    </table>
    ${subfolders.length || pages.length ? nothing : html`<p class="muted">${query ? "Nothing matches." : "Nothing in this folder yet."}</p>`}
  `);
}

const langs = (e: Entry) => e.files.map((f) => html`<span class="badge lang${f.draft ? " draft" : ""}" title=${f.draft ? "draft: true" : f.path}>${f.lang}</span>`);

/** The badge of a page's draft, or of a page not saved yet. */
function badgeOf(e: Entry | undefined): TemplateResult | typeof nothing {
  if (e?.isNew) return html`<span class="badge pending">new</span>`;
  const draft = e ? draftOf(e.key) : null;
  return draft ? html`<a class="badge pending" href="#/d/${draft.id}">draft</a>` : nothing;
}

/** A folder in the list: its pages below it (in a taxonomy's folder, its terms), and its own
 * page's languages. */
function folderRow(key: string, title: string, where: string, entries: Entry[]): TemplateResult {
  const own = entries.find((e) => e.key === `${key}/_index`);
  const below = entries.filter((e) => e.key.startsWith(`${key}/`) && e !== own);
  const pages = below.filter((e) => e.kind === "page").length;
  const terms = taxonomyOf(key) ? below.filter((e) => ownFolder(e.key) !== null).length : 0;
  return html`
    <tr class="folder">
      <td>
        <a href=${folderHref(key)}><svg class="folder-icon" viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 3.5h4.5l1.5 1.5h7v7.5h-13z" /></svg>${title}</a>
        <span class="muted">${holds(key, pages, terms)}</span>
        ${where ? html`<small class="muted where">in ${where}</small>` : nothing}
      </td>
      <td>${own ? langs(own) : nothing}</td>
      <td>${badgeOf(own)}</td>
    </tr>
  `;
}

function pageRow(e: Entry, where: string): TemplateResult {
  return html`
    <tr>
      <td>
        <a href="#/e/${encodeURIComponent(e.key)}">${e.title}</a>
        ${where ? html`<small class="muted where">in ${where}</small>` : nothing}
      </td>
      <td>${langs(e)}</td>
      <td>${badgeOf(e)}</td>
    </tr>
  `;
}
