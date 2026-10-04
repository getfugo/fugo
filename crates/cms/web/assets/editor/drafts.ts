// The drafts: their list, and a draft's changes as a diff.

import { api, messageOf } from "./api";
import { discard, publish } from "./changes";
import { type Draft, type DraftDetail } from "./data";
import { h, render, toast } from "./dom";
import { drafts, me, pending, site } from "./state";

export function draftList(list: Draft[]): HTMLElement {
  if (list.length === 0) return h("p", { class: "muted" }, "No drafts.");
  return h(
    "table",
    { class: "entries" },
    h(
      "tbody",
      {},
      list.map((d) =>
        h(
          "tr",
          {},
          h("td", {}, h("a", { href: `#/d/${d.id}` }, d.title || d.entry)),
          h("td", { class: "muted" }, d.author?.email ?? ""),
          h("td", { class: "muted" }, d.updated ? new Date(d.updated).toLocaleString() : ""),
        ),
      ),
    ),
  );
}

export function draftsView(): void {
  render(h("h1", {}, "Drafts"), draftList(drafts));
}

export async function draftView(id: string): Promise<void> {
  render(h("p", { class: "muted" }, "Loading…"));
  let d: DraftDetail;
  try {
    d = await api<DraftDetail>("GET", "draft", { id });
  } catch (e) {
    toast(messageOf(e), "error");
    return draftsView();
  }
  const known = site.entries.some((e) => e.key === d.entry) || pending.has(d.entry);
  render(
    h("div", { class: "crumbs" }, h("a", { href: "#/drafts" }, "Drafts"), " / ", d.entry),
    h("h1", {}, d.title || d.entry),
    d.conflicts.length
      ? h("p", { class: "warn" }, `The site changed ${d.conflicts.join(", ")} since this draft was made: open the page, redo the changes, and discard this draft.`)
      : null,
    h("h2", {}, "Changes"),
    d.files.map((f) =>
      h(
        "details",
        { class: "change", open: d.files.length <= 3 || undefined },
        h("summary", {}, h("span", { class: `badge ${f.status}` }, f.status), " ", h("code", {}, f.previous ? `${f.previous} → ${f.path}` : f.path)),
        f.patch ? diff(f.patch) : h("p", { class: "muted" }, "A binary file, or too large to show."),
      ),
    ),
    h("h2", {}, "Saves"),
    h(
      "ul",
      {},
      d.commits.map((c) => h("li", {}, h("span", { class: "muted" }, c.author?.email ?? ""), " — ", c.subject)),
    ),
    h(
      "div",
      { class: "actions" },
      known ? h("a", { class: "button", href: `#/e/${encodeURIComponent(d.entry)}` }, "Open the page") : null,
      me.publish ? h("button", { class: "primary", disabled: d.conflicts.length > 0 || undefined, onclick: () => publish(id) }, "Publish") : null,
      h("button", { class: "danger", onclick: () => discard(id) }, "Discard"),
    ),
  );
}

function diff(patch: string): HTMLElement {
  return h(
    "pre",
    { class: "diff" },
    patch
      .split("\n")
      .map((line) =>
        h("span", { class: line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : line.startsWith("@@") ? "hunk" : undefined }, `${line}\n`),
      ),
  );
}
