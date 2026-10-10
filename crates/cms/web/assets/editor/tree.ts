// The sidebar: the sections, each with its folders below it as a tree, at any depth. A folder
// opens and closes in place, without leaving the view shown; the folder shown (or the one whose
// list has the page shown) is marked, and it and the folders above it open by themselves. The
// terms that lists show as pages are not folders of the tree.

import { html, nothing, type TemplateResult } from "lit-html";
import { listedIn, termFolders } from "./browse";
import { type Entry } from "./data";
import { drawSidebar } from "./dom";
import { allFolders, folderHref, folderTitle, parentDir } from "./folders";
import { allEntries, site } from "./state";
import { holds } from "./views";

interface Node {
  key: string;
  title: string;
}

/** The folders opened or closed by hand, over those that open by themselves. */
const chosen = new Map<string, boolean>();
/** The folder marked when the sidebar was last drawn, and the one it last scrolled to. */
let lastShown: string | null = null;
let revealed: string | null = null;

/** The folder shown, or the one whose list has the page shown; none on the other views. */
function shownFolder(): string | null {
  const at = /^#\/([se])\/(.*)$/.exec(location.hash);
  if (!at) return null;
  const path = decodeURIComponent(at[2]);
  return at[1] === "s" ? path.replace(/^\/+|\/+$/g, "") : listedIn(path);
}

/** The folders of the tree below each folder, sorted by title (the sections' are the sections). */
function childrenOf(entries: Entry[]): Map<string, Node[]> {
  const terms = termFolders(entries);
  const titles = new Map(entries.filter((e) => e.key.endsWith("/_index")).map((e) => [e.key.slice(0, -"/_index".length), e.title]));
  const out = new Map<string, Node[]>();
  for (const key of allFolders(entries)) {
    const parent = parentDir(key);
    if (!parent || terms.has(key)) continue;
    const node = { key, title: titles.get(key) || folderTitle(key, entries) };
    out.set(parent, [...(out.get(parent) ?? []), node]);
  }
  for (const nodes of out.values()) nodes.sort((a, b) => a.title.localeCompare(b.title));
  return out;
}

/** Scrolls the sidebar, and only it, to the marked folder: when the mark has moved (not when a
 * folder is opened, or the view drawn again) and its row is out of sight. */
export function reveal(nav: HTMLElement): void {
  if (lastShown === revealed) return;
  revealed = lastShown;
  const row = nav.querySelector<HTMLElement>(".node.active");
  if (!row) return;
  const top = row.getBoundingClientRect().top - nav.getBoundingClientRect().top + nav.scrollTop;
  if (top < nav.scrollTop || top + row.offsetHeight > nav.scrollTop + nav.clientHeight) nav.scrollTop = Math.max(0, top - nav.clientHeight / 3);
}

export function sidebar(): TemplateResult {
  const children = childrenOf(allEntries());
  let shown = shownFolder();
  // A folder the tree does not have (a term's) marks the folder above it.
  const inTree = (dir: string) => site.sections.some((s) => s.key === dir) || (children.get(parentDir(dir))?.some((n) => n.key === dir) ?? false);
  while (shown && !inTree(shown)) shown = parentDir(shown);
  // Where the view goes, the folders open again down to it.
  if (shown !== lastShown) {
    for (let dir = shown; dir; dir = parentDir(dir)) chosen.delete(dir);
    lastShown = shown;
  }
  const isOpen = (dir: string) => chosen.get(dir) ?? (shown !== null && (shown === dir || shown.startsWith(`${dir}/`)));

  const item = (node: Node, depth: number, count?: number, size?: string): TemplateResult => {
    const below = node.key ? (children.get(node.key) ?? []) : [];
    const open = below.length > 0 && isOpen(node.key);
    const here = node.key === shown;
    const toggle = () => {
      chosen.set(node.key, !open);
      drawSidebar();
    };
    return html`
      <li>
        <div class="node${here ? " active" : ""}" style="--depth: ${depth}">
          ${below.length
            ? html`<button class="twisty" aria-expanded=${open ? "true" : "false"} aria-label=${node.title} title=${open ? "Close" : "Open"} @click=${toggle}>
                <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3.5l4.5 4.5-4.5 4.5" /></svg>
              </button>`
            : html`<span class="twisty"></span>`}
          <a href=${folderHref(node.key)} class=${here ? "active" : nothing} aria-current=${here ? "page" : nothing} title=${size ?? node.title}>
            <span class="name">${node.title}</span>${count === undefined ? nothing : html`<span class="count">${count}</span>`}
          </a>
        </div>
        ${open ? html`<ul>${below.map((n) => item(n, depth + 1))}</ul>` : nothing}
      </li>
    `;
  };

  return html`
    <h2>Sections</h2>
    <ul class="tree">
      ${site.sections.map((s) => item({ key: s.key, title: s.title }, 0, s.count || s.folders, holds(s.key, s.count, s.folders)))}
    </ul>
  `;
}
