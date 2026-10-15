// The open page: its documents (one per language) and their editors.

import { html, nothing, type TemplateResult } from "lit-html";
import { live } from "lit-html/directives/live.js";
import { clone, decode, encode, join, split } from "../codec";
import { base64ToText } from "../common";
import { api, ApiError, messageOf } from "./api";
import { crumbs, listedIn } from "./browse";
import { addTranslation, deletePage, discard, movePage, publish, save } from "./changes";
import { type Doc } from "./data";
import { loading, show, toast, valueOf } from "./dom";
import { bodyEditor, filesPanel } from "./files";
import { foldersOf } from "./folders";
import { fieldsForm } from "./form";
import { icon } from "./icons";
import { type MdOutlinedSelect, type MdSwitch } from "./material";
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
  show(loading());
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
    <div class="crumbs">${crumbs(listedIn(entry.key))}</div>
    <div class="title-row">
      <h1>${title}</h1>
      ${draft ? html`<a class="badge pending" href="#/d/${draft.id}">has a draft</a>` : nothing}
    </div>
    <div class="tabs">
      <md-tabs aria-label="Languages">
        ${[...docs.keys()].map((lang) => html`<md-primary-tab class="tab" ?active=${lang === p.lang} @click=${() => openLang(lang)}>${langName(lang)}</md-primary-tab>`)}
      </md-tabs>
      ${missing.map((l) => html`<md-text-button class="add" title="Add a ${l.name} version" @click=${() => addTranslation(l.key)}>${icon("add", "icon")}${l.name}</md-text-button>`)}
    </div>
    ${doc ? docEditor(doc) : html`<p class="muted">This page has no file in this language yet.</p>`}
    ${entry.bundle || site.media ? filesPanel() : nothing}
    <div class="actions sticky">
      ${editable
        ? html`<md-filled-button class="primary" @click=${save}>${me.workflow === "review" ? "Save draft" : "Save and publish"}</md-filled-button>`
        : html`<span class="muted">You may not change this page.</span>`}
      ${draft && me.publish ? html`<md-filled-tonal-button @click=${() => publish(draft.id)}>${icon("publish", "icon")}Publish draft</md-filled-tonal-button>` : nothing}
      ${draft ? html`<md-outlined-button class="danger" @click=${() => discard(draft.id)}>Discard draft</md-outlined-button>` : nothing}
      <span class="spacer"></span>
      ${editable ? moveControl() : nothing}
      ${editable && !entry.isNew ? html`<md-text-button class="danger" @click=${deletePage}>${icon("delete", "icon")}Delete page</md-text-button>` : nothing}
    </div>
  `);
}

/** Moving a saved page to another folder of its section, at any depth (none in a section without
 * folders): to the folder chosen, once the move is confirmed. */
function moveControl(): TemplateResult | typeof nothing {
  const { entry } = current();
  if (entry.kind !== "page" || entry.isNew || !entry.section) return nothing;
  const section = sectionFor(entry.section);
  const folders = foldersOf(section, allEntries());
  if (folders.length === 0) return nothing;
  const here = entry.key.includes("/") ? entry.key.slice(0, entry.key.lastIndexOf("/")) : "";
  const change = (ev: Event) => {
    const select = ev.target as MdOutlinedSelect;
    const folder = select.value;
    // Back to none: a move that is not confirmed leaves nothing chosen.
    select.reset();
    if (folder) void movePage(folder);
  };
  return html`
    <md-outlined-select class="move" label="Move to" @change=${change}>
      ${[{ key: section.key, title: section.title, depth: 0 }, ...folders].map(
        (f) => html`
          <md-select-option value=${f.key} ?disabled=${f.key === here}>
            <div slot="headline" style="padding-inline-start: ${f.depth}rem">${f.title}</div>
          </md-select-option>
        `,
      )}
    </md-outlined-select>
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
        <md-switch
          .selected=${live(doc.raw)}
          ?disabled=${readOnly || (doc.error !== null && doc.raw)}
          @change=${(ev: Event) => toggleRaw(doc, (ev.target as MdSwitch).selected)}
        ></md-switch>
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
