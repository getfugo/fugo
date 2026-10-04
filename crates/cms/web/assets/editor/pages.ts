// The open page: its documents (one per language) and their editors.

import { clone, decode, encode, join, split } from "../codec";
import { base64ToText } from "../common";
import { api, ApiError, messageOf } from "./api";
import { addTranslation, deletePage, discard, publish, save } from "./changes";
import { type Doc } from "./data";
import { h, render, toast, valueOf } from "./dom";
import { bodyEditor, filesPanel } from "./files";
import { fieldsForm } from "./form";
import { canEdit, current, draftOf, langName, me, pending, setPage, site } from "./state";
import { notFound } from "./views";

/** A document of the open page: one language's file. */
export function makeDoc(path: string, text: string, sha: string | null, isNew = false): Doc {
  const parts = split(text);
  const doc: Doc = { path, sha, isNew, parts, original: null, data: null, body: parts.body, error: null, raw: false, rawText: text, initial: text };
  try {
    doc.original = decode(parts);
    doc.data = clone(doc.original);
  } catch (e) {
    doc.error = messageOf(e);
    doc.raw = true;
  }
  return doc;
}

/** The file text of a document as it is now. */
export function docText(doc: Doc): string {
  if (doc.raw || doc.original === null || doc.data === null) return doc.rawText;
  return join({ ...encode(doc.parts, doc.original, doc.data), body: doc.body });
}

export async function openPage(key: string): Promise<void> {
  const entry = site.entries.find((e) => e.key === key) ?? pending.get(key);
  if (!entry) return notFound();
  render(h("p", { class: "muted" }, "Loading…"));
  const draft = draftOf(key);
  const docs = new Map<string, Doc>();
  try {
    await Promise.all(
      entry.files.map(async (f) => {
        if (f.doc) {
          docs.set(f.lang, f.doc);
          return;
        }
        try {
          const file = await api<{ content: string; sha: string }>("GET", "file", { path: f.path, draft: draft?.id });
          docs.set(f.lang, makeDoc(f.path, base64ToText(file.content), file.sha));
        } catch (e) {
          if (!(e instanceof ApiError) || e.status !== 404) throw e;
        }
      }),
    );
  } catch (e) {
    toast(messageOf(e), "error");
  }
  const langs = site.languages.map((l) => l.key).filter((l) => docs.has(l));
  setPage({ entry, draft, docs, uploads: [], deletes: new Set(), lang: langs[0] ?? site.default_language });
  pageView();
}

export function pageView(): void {
  const p = current();
  const { entry, draft, docs } = p;
  const section = site.sections.find((s) => s.key === entry.section);
  const doc = docs.get(p.lang);
  const missing = site.languages.filter((l) => !docs.has(l.key));
  const editable = [...docs.values()].some((d) => canEdit(d.path));
  const title = doc?.data?.title;
  render(
    h("div", { class: "crumbs" }, h("a", { href: `#/s/${encodeURIComponent(entry.section)}` }, section?.title ?? "Pages"), " / ", entry.key),
    h(
      "div",
      { class: "title-row" },
      h("h1", {}, typeof title === "string" && title ? title : entry.title),
      draft ? h("a", { class: "badge pending", href: `#/d/${draft.id}` }, "has a draft") : null,
    ),
    h(
      "div",
      { class: "tabs" },
      [...docs.keys()].map((lang) =>
        h(
          "button",
          {
            class: lang === p.lang ? "tab active" : "tab",
            onclick: () => {
              p.lang = lang;
              pageView();
            },
          },
          langName(lang),
        ),
      ),
      missing.map((l) => h("button", { class: "tab add", onclick: () => addTranslation(l.key), title: `Add a ${l.name} version` }, `+ ${l.name}`)),
    ),
    doc ? docEditor(doc) : h("p", { class: "muted" }, "This page has no file in this language yet."),
    entry.bundle || site.media ? filesPanel() : null,
    h(
      "div",
      { class: "actions sticky" },
      editable
        ? h("button", { class: "primary", onclick: save }, me.workflow === "review" ? "Save draft" : "Save and publish")
        : h("span", { class: "muted" }, "You may not change this page."),
      draft && me.publish ? h("button", { onclick: () => publish(draft.id) }, "Publish draft") : null,
      draft ? h("button", { class: "danger", onclick: () => discard(draft.id) }, "Discard draft") : null,
      editable && !entry.isNew ? h("button", { class: "danger subtle", onclick: deletePage }, "Delete page") : null,
    ),
  );
}

function docEditor(doc: Doc): HTMLElement {
  const readOnly = !canEdit(doc.path);
  const head = h(
    "div",
    { class: "doc-head" },
    h("code", {}, doc.path),
    readOnly ? h("span", { class: "badge" }, "read only") : null,
    h("span", { class: "spacer" }),
    h(
      "label",
      { class: "toggle" },
      h("input", {
        type: "checkbox",
        checked: doc.raw,
        disabled: readOnly || (doc.error !== null && doc.raw) ? true : undefined,
        onchange: (ev: Event) => toggleRaw(doc, (ev.target as HTMLInputElement).checked),
      }),
      " Edit as text",
    ),
  );
  if (doc.raw || doc.data === null) {
    return h(
      "section",
      { class: "doc" },
      head,
      doc.error ? h("p", { class: "warn" }, `The front matter could not be read (${doc.error}); edit the file as text.`) : null,
      h("textarea", {
        class: "raw",
        rows: 30,
        spellcheck: "false",
        readonly: readOnly || undefined,
        value: doc.rawText,
        oninput: (ev: Event) => {
          doc.rawText = valueOf(ev);
        },
      }),
    );
  }
  return h("section", { class: "doc" }, head, fieldsForm(doc.data, readOnly), bodyEditor(doc, readOnly));
}

function toggleRaw(doc: Doc, raw: boolean): void {
  if (raw) {
    doc.rawText = docText({ ...doc, raw: false });
    doc.raw = true;
  } else {
    const next = makeDoc(doc.path, doc.rawText, doc.sha, doc.isNew);
    if (next.error) {
      toast(`The front matter has an error: ${next.error}`, "error");
      return pageView();
    }
    // The original stays as it was loaded, so unchanged keys keep their text.
    Object.assign(doc, { parts: next.parts, data: next.data, body: next.body, raw: false, error: null });
    if (doc.original === null) doc.original = next.original;
  }
  pageView();
}
