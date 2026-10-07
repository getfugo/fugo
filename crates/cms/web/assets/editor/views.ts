// The layout (header and sidebar) and the views of the site and its sections.

import { html, nothing, type TemplateResult } from "lit-html";
import { askNewPage, newFolder, newPage } from "./changes";
import { type Entry, type Section } from "./data";
import { show, valueOf } from "./dom";
import { draftList } from "./drafts";
import { type Folder, foldersOf } from "./folders";
import { draftOf, drafts, me, pending, site } from "./state";

export function header(): TemplateResult {
  return html`
    <header>
      <a class="brand" href="#/">${site.title}</a>
      <a class="site" href=${site.site_url} target="_blank" rel="noopener">View site ↗</a>
      <a class="licences" href="cms.js.LEGAL.txt" target="_blank" title="The licences of the libraries this editor uses">Licences</a>
      <span class="spacer"></span>
      ${me.workflow === "review" ? html`<a href="#/drafts" class="drafts-link">Drafts (${drafts.length})</a>` : nothing}
      <span class="user" title="Roles: ${me.roles.join(", ")}">${me.email}</span>
    </header>
  `;
}

export function sidebar(): TemplateResult {
  const selected = decodeURIComponent(/^#\/s\/(.*)$/.exec(location.hash)?.[1] ?? "");
  return html`
    <nav class="sidebar">
      <h2>Sections</h2>
      <ul>
        ${site.sections.map(
          (s) => html`
            <li>
              <a href="#/s/${encodeURIComponent(s.key)}" class=${s.key === selected ? "active" : nothing} title=${sizeOf(s)}>${s.title}<span class="count">${s.count || s.folders}</span></a>
            </li>
          `,
        )}
      </ul>
    </nav>
  `;
}

// ── Views ──────────────────────────────────────────────────────────────────────────────────────

export function home(): void {
  const mine = drafts.filter((d) => d.author?.email?.toLowerCase() === me.email);
  const how =
    me.workflow === "direct"
      ? "Your changes go to the site when you save them."
      : me.publish
        ? "Your changes are saved as drafts; you can publish drafts."
        : "Your changes are saved as drafts; someone who may publish puts them on the site.";
  show(html`
    <h1>${site.title}</h1>
    <p class="muted">Signed in as ${me.email} (${me.roles.join(", ")}). ${how}</p>
    ${mine.length ? html`<h2>Your drafts</h2>${draftList(mine)}` : nothing}
    <h2>Sections</h2>
    <div class="cards">
      ${site.sections.map(
        (s) => html`
          <a class="card" href="#/s/${encodeURIComponent(s.key)}"><strong>${s.title}</strong><span class="muted">${sizeOf(s)}</span></a>
        `,
      )}
    </div>
  `);
}

/** What a section holds: its pages and its folders, which in a taxonomy's section are its terms
 * (`52 brands`). The sidebar shows the pages, or the folders of a section without pages. */
function sizeOf(s: Section): string {
  const taxonomy = site.taxonomies.find((t) => t.plural === s.key);
  const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const parts = s.count || !s.folders ? [count(s.count, "page", "pages")] : [];
  if (s.folders) parts.push(taxonomy ? count(s.folders, taxonomy.singular, taxonomy.plural) : count(s.folders, "folder", "folders"));
  return parts.join(" · ");
}

/** What the section view shows besides its pages: the form for a new page or folder, and the
 * filter. It stays while the view is redrawn, until another section is shown. */
const sectionState: { key: string; form: "" | "page" | "folder"; filter: string } = { key: "", form: "", filter: "" };

export function sectionView(key: string): void {
  const section = site.sections.find((s) => s.key === key);
  if (!section) return notFound();
  if (sectionState.key !== key) Object.assign(sectionState, { key, form: "", filter: "" });
  const redraw = () => sectionView(key);
  const toggle = (form: "page" | "folder") => {
    sectionState.form = sectionState.form === form ? "" : form;
    redraw();
  };
  const entries = site.entries
    .filter((e) => e.section === key)
    .sort((a, b) => (a.kind === b.kind ? a.title.localeCompare(b.title) : a.kind === "page" ? 1 : -1));
  // A section with folders of its own asks which one a new page goes in; any section but the
  // root can have a new folder (an index page in a new directory).
  const folders = foldersOf(section, [...site.entries, ...pending.values()]);
  const query = sectionState.filter.trim().toLowerCase();
  show(html`
    <div class="title-row">
      <h1>${section.title}</h1>
      <button class="primary" @click=${() => (folders.length ? toggle("page") : askNewPage(section))}>New page</button>
      ${section.key ? html`<button @click=${() => toggle("folder")}>New folder</button>` : nothing}
    </div>
    ${sectionState.form === "page" ? createForm(section, folders, "Create page", (title, folder) => newPage(section, title, folder)) : nothing}
    ${sectionState.form === "folder" ? createForm(section, folders, "Create folder", (title, folder) => newFolder(section, title, folder)) : nothing}
    <input
      type="search"
      class="filter"
      placeholder="Filter by title or path"
      .value=${sectionState.filter}
      @input=${(ev: Event) => {
        sectionState.filter = valueOf(ev);
        redraw();
      }}
    />
    <table class="entries">
      <thead>
        <tr><th>Title</th><th>Languages</th><th></th></tr>
      </thead>
      <tbody>
        ${entries.map((e) => entryRow(e, query))}
      </tbody>
    </table>
  `);
}

/** A page of the section's list, hidden when it does not match the filter's `query`. */
function entryRow(e: Entry, query: string): TemplateResult {
  const draft = draftOf(e.key);
  const hidden = query !== "" && !`${e.title} ${e.key}`.toLowerCase().includes(query);
  return html`
    <tr ?hidden=${hidden}>
      <td>
        <a href="#/e/${encodeURIComponent(e.key)}">${e.title}</a>
        ${e.kind !== "page" ? html`<span class="badge">section page</span>` : nothing}
      </td>
      <td>
        ${e.files.map((f) => html`<span class="badge lang${f.draft ? " draft" : ""}" title=${f.draft ? "draft: true" : f.path}>${f.lang}</span>`)}
      </td>
      <td>${draft ? html`<a class="badge pending" href="#/d/${draft.id}">draft</a>` : nothing}</td>
    </tr>
  `;
}

/** A form for something new in `section`: the folder it goes in (the section's own or one of
 * `folders`) and its title. */
function createForm(section: Section, folders: Folder[], label: string, create: (title: string, folder: string) => void): TemplateResult {
  const submit = (ev: Event) => {
    ev.preventDefault();
    const data = new FormData(ev.currentTarget as HTMLFormElement);
    create(String(data.get("title") ?? "").trim(), String(data.get("folder") ?? ""));
  };
  return html`
    <form class="new-page" @submit=${submit}>
      <select name="folder" required aria-label="Folder">
        <option value="" disabled selected>Folder…</option>
        <option value=${section.key}>${section.title}</option>
        ${folders.map((f) => html`<option value=${f.key}>${"\u00a0\u00a0".repeat(f.depth)}${f.title}</option>`)}
      </select>
      <input name="title" type="text" required placeholder="Title" aria-label="Title" />
      <button class="primary" type="submit">${label}</button>
    </form>
  `;
}

export function notFound(): void {
  show(html`
    <h1>Not found</h1>
    <p><a href="#/">Back to the start</a></p>
  `);
}
