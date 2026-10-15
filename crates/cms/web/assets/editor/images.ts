// The open page's images: the files an image field may name, the address that shows each (where
// the site publishes the page's files), their pictures, and the image field: a picture of the file
// chosen, a select of the page's images, and a button that uploads one and chooses it.

import { html, nothing, type TemplateResult } from "lit-html";
import { keyed } from "lit-html/directives/keyed.js";
import { live } from "lit-html/directives/live.js";
import { createRef, ref } from "lit-html/directives/ref.js";
import { extensionOf } from "../common";
import { valueOf } from "./dom";
import { bundleDir, upload } from "./files";
import { type Setter } from "./form";
import { icon } from "./icons";
import { type ImageUrl } from "./markdown";
import { pageView } from "./pages";
import { canEdit, current, site } from "./state";

const MIME: Record<string, string> = {
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  png: "image/png",
  gif: "image/gif",
  webp: "image/webp",
  svg: "image/svg+xml",
  avif: "image/avif",
};

export const mimeOf = (path: string) => MIME[extensionOf(path)] ?? "application/octet-stream";

/** Whether the file at `path` is an image a browser shows. */
export const isImage = (path: string) => extensionOf(path) in MIME;

/**
 * The URL path, below the site's root, of the page the site publishes the open page's file `path`
 * with: the page in the file's language (a name without one of the site's languages, `photo.jpg`,
 * is the default language's), else the page in the language shown. A translation does not get
 * its own copy of the files.
 */
function publishedWith(path: string): string {
  const p = current();
  const parts = (path.split("/").pop() ?? "").split(".");
  const named = parts.length > 2 ? parts[parts.length - 2] : "";
  const lang = site.languages.some((l) => l.key === named) ? named : site.default_language;
  const url = (l: string) => p.entry.files.find((f) => f.lang === l)?.url;
  return url(lang) ?? url(p.lang) ?? "/";
}

/** The address that shows the image `src` of the open page (in its text or its front matter): a
 * file uploaded and not saved yet from the browser's copy, a file of its bundle where the site
 * publishes it, else `src` relative to the page in the language shown. */
export function imageUrl(): ImageUrl {
  const p = current();
  const root = new URL(site.site_url).pathname.replace(/\/$/, "");
  const at = (url: string) => new URL(root + url, location.origin).href;
  const own = at(p.entry.files.find((f) => f.lang === p.lang)?.url ?? "/");
  const dir = p.entry.bundle ? bundleDir() : null;
  return (src) => {
    const name = src.split("/").pop();
    const upload = p.uploads.find((u) => u.path.split("/").pop() === name);
    if (upload) return `data:${mimeOf(upload.path)};base64,${upload.content}`;
    try {
      const file = dir && !/^([a-z][a-z0-9+.-]*:|\/)/i.test(src) ? `${dir}/${src.replace(/^\.\//, "")}` : null;
      if (file && p.entry.resources.includes(file)) return new URL(src, at(publishedWith(file))).href;
      return new URL(src, own).href;
    } catch {
      return src;
    }
  };
}

/** What an image field may name: the bundle's images (relative to the bundle), and images
 * uploaded into the media directory (as `media_ref` names them). */
export function imageChoices(): string[] {
  const p = current();
  const out: string[] = [];
  if (p.entry.bundle) {
    const dir = bundleDir();
    for (const f of [...p.entry.resources, ...p.uploads.map((u) => u.path)]) {
      if (f.startsWith(`${dir}/`) && !p.deletes.has(f) && isImage(f)) out.push(f.slice(dir.length + 1));
    }
  }
  const media = site.media;
  const ref = site.media_ref;
  if (media && ref !== null) {
    for (const u of p.uploads) {
      if (u.path.startsWith(`${media}/`) && isImage(u.path)) out.push(ref + u.path.slice(media.length + 1));
    }
  }
  return out;
}

/** Where the open page's uploads go: its bundle, else the media directory (none: nowhere). */
export function uploadDir(): string | null {
  return current().entry.bundle ? bundleDir() : site.media;
}

// A picture that does not load (a file not published yet) shows the placeholder's icon instead.
const failed = (ev: Event) => (ev.target as Element).closest(".thumb")?.classList.add("missing");
const loaded = (ev: Event) => (ev.target as Element).closest(".thumb")?.classList.remove("missing");

/** The picture of the image at `src` named `name` (none: an empty frame), which opens the image
 * in a new tab when the site serves it. */
export function thumbnail(src: string | null, name: string): TemplateResult {
  if (!src) return html`<span class="thumb empty">${icon("image")}</span>`;
  const img = html`${icon("image")}<img src=${src} alt=${name} loading="lazy" @error=${failed} @load=${loaded} />`;
  return src.startsWith("data:")
    ? html`<span class="thumb">${img}</span>`
    : html`<a class="thumb" href=${src} target="_blank" rel="noopener" title="Open ${name}">${img}</a>`;
}

/** An image field: the picture of the file chosen, a select of the page's images (each with its
 * picture), and a button that uploads an image into the page's folder and chooses it. */
export function imageInput(value: string, set: Setter, readOnly: boolean, id: string): TemplateResult {
  const names = imageChoices();
  const url = imageUrl();
  const p = current();
  const dir = uploadDir();
  // A field names a file of the media directory as `media_ref` does: without it, none.
  const named = p.entry.bundle || site.media_ref !== null;
  const types = site.upload_types.filter((t) => t in MIME);
  const canUpload = !readOnly && dir && named && types.length && canEdit(`${dir}/upload.${types[0]}`);
  const picker = createRef<HTMLInputElement>();
  const choose = (ev: Event) => {
    set(valueOf(ev));
    pageView();
  };
  const pick = async () => {
    const input = picker.value;
    const file = input?.files?.[0];
    if (!input || !file || !dir) return;
    input.value = "";
    const [path] = await upload([file], dir);
    if (path) set(p.entry.bundle ? path.slice(dir.length + 1) : (site.media_ref ?? "") + path.slice(dir.length + 1));
    pageView();
  };
  const option = (v: string) => html`
    <md-select-option value=${v} .selected=${live(v === value)}>
      ${v ? html`<img slot="start" class="option-thumb" src=${url(v)} alt="" loading="lazy" />` : nothing}
      <div slot="headline">${v || "—"}</div>
    </md-select-option>
  `;
  const all = ["", ...(value && !names.includes(value) ? [value, ...names] : names)];
  // A new select for new choices: one that gets an option chosen already (an image uploaded here)
  // shows no text for it.
  return html`
    <div class="image-input">
      ${thumbnail(value ? url(value) : null, value)}
      ${keyed(
        all.join("\n"),
        html`<md-outlined-select id=${id} ?disabled=${readOnly} @change=${choose}>${all.map(option)}</md-outlined-select>`,
      )}
      ${canUpload
        ? html`
            <md-outlined-button @click=${() => picker.value?.click()}>${icon("upload", "icon")}Upload…</md-outlined-button>
            <input ${ref(picker)} type="file" accept=${types.map((t) => `.${t}`).join(",")} hidden @change=${pick} />
          `
        : nothing}
    </div>
  `;
}
