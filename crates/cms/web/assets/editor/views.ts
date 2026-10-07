// The layout (header and sidebar) and the start page. A section's view is that of its folder
// (browse.ts).

import { html, nothing, type TemplateResult } from "lit-html";
import { askNewFolder, mayAddFolder } from "./create";
import { type Section } from "./data";
import { show } from "./dom";
import { draftList } from "./drafts";
import { drafts, me, site } from "./state";

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
  // The section of the folder or page shown (a page at the top is the root's).
  const at = /^#\/([se])\/(.*)$/.exec(location.hash);
  const path = at ? decodeURIComponent(at[2]) : null;
  const selected = path === null ? null : at?.[1] === "s" || path.includes("/") ? path.split("/")[0] : "";
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
    <div class="title-row">
      <h2>Sections</h2>
      ${mayAddFolder("") ? html`<button @click=${() => askNewFolder("")}>New section</button>` : nothing}
    </div>
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

export function notFound(): void {
  show(html`
    <h1>Not found</h1>
    <p><a href="#/">Back to the start</a></p>
  `);
}
