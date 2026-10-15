// The header and the start page. The sidebar is tree.ts; a section's view is that of its folder
// (browse.ts).

import { html, nothing, type TemplateResult } from "lit-html";
import { messageOf, signOut } from "./api";
import { askNewFolder, mayAddFolder } from "./create";
import { type Section } from "./data";
import { show, toast } from "./dom";
import { draftList } from "./drafts";
import { icon } from "./icons";
import { drafts, me, site, taxonomyOf } from "./state";

/** The bar at the top: the site's name, then the drafts, the site, and who is signed in. */
export function header(): TemplateResult {
  return html`
    <header>
      <a class="brand" href="#/">${site.title}</a>
      <span class="spacer"></span>
      ${me.workflow === "review" ? html`<md-text-button class="drafts-link" href="#/drafts">Drafts (${drafts.length})</md-text-button>` : nothing}
      <md-text-button class="site" href=${site.site_url} target="_blank" trailing-icon>View site${icon("open_in_new", "icon")}</md-text-button>
      <span class="user" title="Roles: ${me.roles.join(", ")}">${me.email}</span>
      ${me.login === "local"
        ? nothing
        : html`<md-text-button class="sign-out" @click=${() => signOut(me.login).catch((e) => toast(messageOf(e), "error"))}>Sign out</md-text-button>`}
      <a class="licences" href="cms.js.LEGAL.txt" target="_blank" title="The licences of the libraries this editor uses">Licences</a>
    </header>
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
  const who = me.login === "local" ? `Editing the git repository on this computer as ${me.email}` : `Signed in as ${me.email}`;
  show(html`
    <h1>${site.title}</h1>
    <p class="muted">${who} (${me.roles.join(", ")}). ${how}</p>
    ${mine.length ? html`<h2>Your drafts</h2>${draftList(mine)}` : nothing}
    <div class="title-row">
      <h2>Sections</h2>
      ${mayAddFolder("") ? html`<md-outlined-button @click=${() => askNewFolder("")}>${icon("add", "icon")}New section</md-outlined-button>` : nothing}
    </div>
    <div class="cards">
      ${site.sections.map(
        (s) => html`
          <a class="card" href="#/s/${encodeURIComponent(s.key)}">
            <md-focus-ring></md-focus-ring><md-ripple></md-ripple>
            <strong>${s.title}</strong><span class="muted">${sizeOf(s)}</span>
          </a>
        `,
      )}
    </div>
  `);
}

/** What a section holds, on its card (and over its name in the sidebar). */
const sizeOf = (s: Section) => holds(s.key, s.count, s.folders);

/** What the folder `dir` holds: `pages` and `folders`, which in a taxonomy's folder are its terms
 * (`3 tags`); the pages only when there are some or no folders. */
export function holds(dir: string, pages: number, folders: number): string {
  const taxonomy = taxonomyOf(dir);
  const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const parts = pages || !folders ? [count(pages, "page", "pages")] : [];
  if (folders) parts.push(taxonomy ? count(folders, taxonomy.singular, taxonomy.plural) : count(folders, "folder", "folders"));
  return parts.join(" · ");
}

export function notFound(): void {
  show(html`
    <h1>Not found</h1>
    <p><a href="#/">Back to the start</a></p>
  `);
}
