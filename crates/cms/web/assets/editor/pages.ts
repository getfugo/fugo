// The open page: its documents (one per language) and their editors.

import { html, nothing, type TemplateResult } from "lit-html";
import { live } from "lit-html/directives/live.js";
import { clone, decode, encode, join, split } from "../codec";
import { base64ToText } from "../common";
import { api, ApiError, messageOf } from "./api";
import { crumbs } from "./browse";
import { addTranslation, deletePage, discard, movePage, publish, save } from "./changes";
import { type Doc } from "./data";
import { show, toast, valueOf } from "./dom";
import { bodyEditor, filesPanel } from "./files";
import { foldersOf, ownFolder, placeOf } from "./folders";
import { fieldsForm } from "./form";
import { schedulePreview } from "./preview";
import { allEntries, canEdit, current, draftOf, langName, me, pending, sectionFor, setPage, site } from "./state";
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
  show(html`<p class="muted">Loading…</p>`);
  const draft = (entry.savedWith ? draftOf(entry.savedWith) : null) ?? draftOf(key);
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

/** Shows the open page; called again after every change to what it shows. */
export function pageView(): void {
  const p = current();
  const { entry, draft, docs } = p;
  const doc = docs.get(p.lang);
  const missing = site.languages.filter((l) => !docs.has(l.key));
  const editable = [...docs.values()].some((d) => canEdit(d.path));
  const title = typeof doc?.data?.title === "string" && doc.data.title ? doc.data.title : entry.title;
  const openLang = (lang: string) => {
    p.lang = lang;
    pageView();
  };
  show(html`
    <div class="crumbs">${crumbs(ownFolder(entry.key) ?? placeOf(entry.key) ?? "")}</div>
    <div class="title-row">
      <h1>${title}</h1>
      ${draft ? html`<a class="badge pending" href="#/d/${draft.id}">has a draft</a>` : nothing}
    </div>
    <div class="tabs">
      ${[...docs.keys()].map((lang) => html`<button class=${lang === p.lang ? "tab active" : "tab"} @click=${() => openLang(lang)}>${langName(lang)}</button>`)}
      ${missing.map((l) => html`<button class="tab add" title="Add a ${l.name} version" @click=${() => addTranslation(l.key)}>+ ${l.name}</button>`)}
    </div>
    ${doc ? docEditor(doc) : html`<p class="muted">This page has no file in this language yet.</p>`}
    ${entry.bundle || site.media ? filesPanel() : nothing}
    <div class="actions sticky">
      ${editable
        ? html`<button class="primary" @click=${save}>${me.workflow === "review" ? "Save draft" : "Save and publish"}</button>`
        : html`<span class="muted">You may not change this page.</span>`}
      ${draft && me.publish ? html`<button @click=${() => publish(draft.id)}>Publish draft</button>` : nothing}
      ${draft ? html`<button class="danger" @click=${() => discard(draft.id)}>Discard draft</button>` : nothing}
      ${editable ? moveControl() : nothing}
      ${editable && !entry.isNew ? html`<button class="danger subtle" @click=${deletePage}>Delete page</button>` : nothing}
    </div>
  `);
}

/** Moving a saved page to another folder of its section, at any depth (none in a section without
 * folders). */
function moveControl(): TemplateResult | typeof nothing {
  const { entry } = current();
  if (entry.kind !== "page" || entry.isNew || !entry.section) return nothing;
  const section = sectionFor(entry.section);
  const folders = foldersOf(section, allEntries());
  if (folders.length === 0) return nothing;
  const here = entry.key.includes("/") ? entry.key.slice(0, entry.key.lastIndexOf("/")) : "";
  const submit = (ev: Event) => {
    ev.preventDefault();
    const folder = new FormData(ev.currentTarget as HTMLFormElement).get("folder");
    if (folder !== null) void movePage(String(folder));
  };
  return html`
    <form class="move" @submit=${submit}>
      <select name="folder" required aria-label="Move to">
        <option value="" disabled selected>Move to…</option>
        ${[{ key: section.key, title: section.title, depth: 0 }, ...folders].map(
          (f) => html`<option value=${f.key} ?disabled=${f.key === here}>${"  ".repeat(f.depth)}${f.title}</option>`,
        )}
      </select>
      <button type="submit">Move</button>
    </form>
  `;
}

function docEditor(doc: Doc): TemplateResult {
  const readOnly = !canEdit(doc.path);
  const head = html`
    <div class="doc-head">
      <code>${doc.path}</code>
      ${readOnly ? html`<span class="badge">read only</span>` : nothing}
      <span class="spacer"></span>
      <label class="toggle">
        <input
          type="checkbox"
          .checked=${live(doc.raw)}
          ?disabled=${readOnly || (doc.error !== null && doc.raw)}
          @change=${(ev: Event) => toggleRaw(doc, (ev.target as HTMLInputElement).checked)}
        />
        Edit as text
      </label>
    </div>
  `;
  if (doc.raw || doc.data === null) {
    return html`
      <section class="doc">
        ${head}
        ${doc.error ? html`<p class="warn">The front matter could not be read (${doc.error}); edit the file as text.</p>` : nothing}
        <textarea
          class="raw"
          rows="30"
          spellcheck="false"
          ?readonly=${readOnly}
          .value=${live(doc.rawText)}
          @input=${(ev: Event) => (doc.rawText = valueOf(ev))}
        ></textarea>
      </section>
    `;
  }
  return html`<section class="doc" @input=${schedulePreview}>${head}${fieldsForm(doc.data, readOnly)}${bodyEditor(doc, readOnly)}</section>`;
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
