// The body editor (rich text, or Markdown) with its preview, and the files of a page bundle
// (uploads).

import { html, nothing, type TemplateResult } from "lit-html";
import { live } from "lit-html/directives/live.js";
import { createRef, ref } from "lit-html/directives/ref.js";
import { bytesToBase64, extensionOf } from "../common";
import { type Doc } from "./data";
import { toast, valueOf } from "./dom";
import { pageView } from "./pages";
import { hidePreview, schedulePreview, showPreview } from "./preview";
import { richEditor } from "./richtext";
import { canEdit, current, site } from "./state";

/** The document whose preview is open, if any. */
let previewed: Doc | null = null;

/** How the text is edited: as it reads, or as Markdown; the browser remembers the choice. */
type Mode = "rich" | "markdown";
const MODE = "cms-text-mode";
let mode: Mode = (() => {
  try {
    return window.localStorage.getItem(MODE) === "markdown" ? "markdown" : "rich";
  } catch {
    return "rich";
  }
})();

export function bodyEditor(doc: Doc, readOnly: boolean): TemplateResult {
  const open = previewed === doc;
  const toggle = () => {
    previewed = open ? null : doc;
    if (!previewed) hidePreview();
    pageView();
  };
  const choose = (next: Mode) => () => {
    mode = next;
    try {
      window.localStorage.setItem(MODE, next);
    } catch {
      // Remembered for this page only.
    }
    pageView();
  };
  const rich = mode === "rich";
  // Fills the frame when the preview opens, and a frame drawn anew while it is open.
  const fill = (frame?: Element) => {
    if (frame && previewed === doc) void showPreview(frame as HTMLIFrameElement, doc);
  };
  const input = (ev: Event) => {
    doc.body = valueOf(ev);
    schedulePreview();
  };
  // The frame is sandboxed without scripts (the text may hold any HTML), on the editor's
  // origin so that the preview updates in place as you type.
  return html`
    <div class=${open ? "body-editor previewing" : "body-editor"}>
      <div class="doc-head">
        <strong>Text</strong>
        <span class="modes" role="group" aria-label="Edit the text">
          <button class=${rich ? "small active" : "small"} aria-pressed=${rich} @click=${choose("rich")}>Rich text</button>
          <button class=${rich ? "small" : "small active"} aria-pressed=${!rich} @click=${choose("markdown")}>Markdown</button>
        </span>
        <span class="spacer"></span>
        <small class="muted">${rich ? "HTML and shortcodes show as their source" : "The preview shows it in the site's own page, without shortcodes"}</small>
        <button class="small" @click=${toggle}>${open ? "Close preview" : "Preview"}</button>
      </div>
      ${rich
        ? richEditor(doc, readOnly)
        : html`<textarea class="body" rows="18" ?readonly=${readOnly} .value=${live(doc.body)} @input=${input}></textarea>`}
      <iframe ${ref(fill)} class="preview" sandbox="allow-same-origin" title="Preview" ?hidden=${!open}></iframe>
    </div>
  `;
}

// ── Bundle files and uploads ───────────────────────────────────────────────────────────────────

export function bundleDir(): string {
  const f = current().entry.files[0];
  return f.path.slice(0, f.path.lastIndexOf("/"));
}

const picker = createRef<HTMLInputElement>();

export function filesPanel(): TemplateResult | typeof nothing {
  const p = current();
  const dir = p.entry.bundle ? bundleDir() : site.media;
  if (!dir) return nothing;
  const files = p.entry.bundle ? [...p.entry.resources] : [];
  const waiting = p.uploads.map((u) => u.path);
  const allowed = canEdit(`${dir}/upload.jpg`);
  const pick = () => {
    const input = picker.value;
    if (!input?.files) return;
    const chosen = Array.from(input.files);
    // So that choosing the same files again is a change too.
    input.value = "";
    void upload(chosen, dir);
  };
  const remove = (f: string) => {
    if (waiting.includes(f)) p.uploads = p.uploads.filter((u) => u.path !== f);
    else p.deletes.add(f);
    pageView();
  };
  return html`
    <section class="files">
      <div class="doc-head">
        <strong>${p.entry.bundle ? "Files of this page" : "Uploads"}</strong>
        <span class="spacer"></span>
        ${allowed ? html`<button class="small" @click=${() => picker.value?.click()}>Upload…</button>` : nothing}
        <input ${ref(picker)} type="file" multiple accept=${site.upload_types.map((t) => `.${t}`).join(",")} hidden @change=${pick} />
      </div>
      <p class="muted">${p.entry.bundle ? `In ${dir}/` : `Into ${dir}/ (name them as ${site.media_ref ?? `${dir}/`}…)`}</p>
      <ul class="file-list">
        ${[...files, ...waiting].map(
          (f) => html`
            <li class=${p.deletes.has(f) ? "deleted" : waiting.includes(f) ? "new" : nothing}>
              <code>${f.slice(dir.length + 1)}</code>
              ${waiting.includes(f) ? html`<span class="badge">not saved</span>` : nothing}
              ${canEdit(f) && !p.deletes.has(f) ? html`<button class="icon" title="Delete" @click=${() => remove(f)}>×</button>` : nothing}
            </li>
          `,
        )}
      </ul>
    </section>
  `;
}

async function upload(files: File[], dir: string): Promise<void> {
  const p = current();
  const limit = site.max_upload;
  for (const file of files) {
    const name = file.name.normalize("NFC").replace(/[\\/]/g, "-").replace(/^\.+/, "");
    const path = `${dir}/${name}`;
    if (!site.upload_types.includes(extensionOf(name))) {
      toast(`${name}: this file type cannot be uploaded`, "error");
      continue;
    }
    if (file.size > limit) {
      toast(`${name} is larger than ${Math.round(limit / 1048576)} MB`, "error");
      continue;
    }
    if (!canEdit(path)) {
      toast(`You may not add ${path}`, "error");
      continue;
    }
    const content = bytesToBase64(new Uint8Array(await file.arrayBuffer()));
    p.uploads = p.uploads.filter((u) => u.path !== path);
    p.uploads.push({ path, content });
  }
  pageView();
}
