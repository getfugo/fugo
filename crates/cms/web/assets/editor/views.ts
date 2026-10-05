// The layout (header and sidebar) and the views of the site and its sections.

import { askNewPage, newFolder, newPage } from "./changes";
import { type Section } from "./data";
import { h, render } from "./dom";
import { draftList } from "./drafts";
import { type Folder, foldersOf } from "./folders";
import { draftOf, drafts, me, pending, site } from "./state";

export function header(): HTMLElement {
  return h(
    "header",
    {},
    h("a", { class: "brand", href: "#/" }, site.title),
    h("a", { class: "site", href: site.site_url, target: "_blank", rel: "noopener" }, "View site ↗"),
    h("a", { class: "licences", href: "cms.js.LEGAL.txt", target: "_blank", title: "The licences of the libraries this editor uses" }, "Licences"),
    h("span", { class: "spacer" }),
    me.workflow === "review" ? h("a", { href: "#/drafts", class: "drafts-link" }, `Drafts (${drafts.length})`) : null,
    h("span", { class: "user", title: `Roles: ${me.roles.join(", ")}` }, me.email),
  );
}

export function sidebar(): HTMLElement {
  const selected = decodeURIComponent(/^#\/s\/(.*)$/.exec(location.hash)?.[1] ?? "");
  return h(
    "nav",
    { class: "sidebar" },
    h("h2", {}, "Sections"),
    h(
      "ul",
      {},
      site.sections.map((s) =>
        h(
          "li",
          {},
          h(
            "a",
            { href: `#/s/${encodeURIComponent(s.key)}`, class: s.key === selected ? "active" : undefined },
            s.title,
            h("span", { class: "count" }, String(s.count)),
          ),
        ),
      ),
    ),
  );
}

// ── Views ──────────────────────────────────────────────────────────────────────────────────────

export function home(): void {
  const mine = drafts.filter((d) => d.author?.email?.toLowerCase() === me.email);
  render(
    h("h1", {}, site.title),
    h(
      "p",
      { class: "muted" },
      `Signed in as ${me.email} (${me.roles.join(", ")}). `,
      me.workflow === "review"
        ? me.publish
          ? "Your changes are saved as drafts; you can publish drafts."
          : "Your changes are saved as drafts; someone who may publish puts them on the site."
        : "Your changes go to the site when you save them.",
    ),
    mine.length ? [h("h2", {}, "Your drafts"), draftList(mine)] : null,
    h("h2", {}, "Sections"),
    h(
      "div",
      { class: "cards" },
      site.sections.map((s) =>
        h("a", { class: "card", href: `#/s/${encodeURIComponent(s.key)}` }, h("strong", {}, s.title), h("span", { class: "muted" }, `${s.count} pages`)),
      ),
    ),
  );
}

export function sectionView(key: string): void {
  const section = site.sections.find((s) => s.key === key);
  if (!section) return notFound();
  const filter = h("input", { type: "search", placeholder: "Filter by title or path", class: "filter" });
  const entries = site.entries
    .filter((e) => e.section === key)
    .sort((a, b) => (a.kind === b.kind ? a.title.localeCompare(b.title) : a.kind === "page" ? 1 : -1));
  const rows = entries.map((e) => {
    const draft = draftOf(e.key);
    return h(
      "tr",
      { "data-text": `${e.title} ${e.key}`.toLowerCase() },
      h("td", {}, h("a", { href: `#/e/${encodeURIComponent(e.key)}` }, e.title), e.kind !== "page" ? h("span", { class: "badge" }, "section page") : null),
      h(
        "td",
        {},
        e.files.map((f) => h("span", { class: `badge lang${f.draft ? " draft" : ""}`, title: f.draft ? "draft: true" : f.path }, f.lang)),
      ),
      h("td", {}, draft ? h("a", { class: "badge pending", href: `#/d/${draft.id}` }, "draft") : null),
    );
  });
  const body = h("tbody", {}, rows);
  // A section with folders of its own asks which one a new page goes in; any section but the
  // root can have a new folder (an index page in a new directory).
  const folders = foldersOf(section, [...site.entries, ...pending.values()]);
  const pageForm = createForm(section, folders, "Create page", (title, folder) => newPage(section, title, folder));
  const folderForm = createForm(section, folders, "Create folder", (title, folder) => newFolder(section, title, folder));
  const table = h(
    "table",
    { class: "entries" },
    h("thead", {}, h("tr", {}, h("th", {}, "Title"), h("th", {}, "Languages"), h("th", {}, ""))),
    body,
  );
  filter.addEventListener("input", () => {
    const q = filter.value.trim().toLowerCase();
    for (const tr of Array.from(body.rows)) tr.hidden = q !== "" && !(tr.dataset.text ?? "").includes(q);
  });
  render(
    h(
      "div",
      { class: "title-row" },
      h("h1", {}, section.title),
      h("button", { class: "primary", onclick: () => (folders.length ? pageForm.toggleAttribute("hidden") : askNewPage(section)) }, "New page"),
      section.key ? h("button", { onclick: () => folderForm.toggleAttribute("hidden") }, "New folder") : null,
    ),
    pageForm,
    folderForm,
    filter,
    table,
  );
}

/** A form for something new in `section`: the folder it goes in (the section's own or one of
 * `folders`) and its title. */
function createForm(section: Section, folders: Folder[], label: string, create: (title: string, folder: string) => void): HTMLFormElement {
  const folder = h(
    "select",
    { required: true, "aria-label": "Folder" },
    h("option", { value: "", disabled: true, selected: true }, "Folder…"),
    h("option", { value: section.key }, section.title),
    folders.map((f) => h("option", { value: f.key }, `${"\u00a0\u00a0".repeat(f.depth)}${f.title}`)),
  );
  const title = h("input", { type: "text", required: true, placeholder: "Title", "aria-label": "Title" });
  return h(
    "form",
    {
      class: "new-page",
      hidden: true,
      onsubmit: (ev: Event) => {
        ev.preventDefault();
        create(title.value.trim(), folder.value);
      },
    },
    folder,
    title,
    h("button", { class: "primary", type: "submit" }, label),
  );
}

export function notFound(): void {
  render(h("h1", {}, "Not found"), h("p", {}, h("a", { href: "#/" }, "Back to the start")));
}
