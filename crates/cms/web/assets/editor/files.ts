// The body editor with its preview, and the files of a page bundle (uploads).

import { bytesToBase64, extensionOf } from "../common";
import { type Doc } from "./data";
import { h, toast, valueOf } from "./dom";
import { pageView } from "./pages";
import { hidePreview, schedulePreview, showPreview } from "./preview";
import { canEdit, current, site } from "./state";

export function bodyEditor(doc: Doc, readOnly: boolean): HTMLElement {
  const area = h("textarea", {
    class: "body",
    rows: 18,
    readonly: readOnly || undefined,
    value: doc.body,
    oninput: (ev: Event) => {
      doc.body = valueOf(ev);
      schedulePreview();
    },
  });
  // Sandboxed without scripts (the text may hold any HTML), on the editor's origin so that the
  // preview updates in place as you type.
  const frame = h("iframe", { class: "preview", sandbox: "allow-same-origin", title: "Preview", hidden: true });
  const button = h("button", { class: "small" }, "Preview");
  button.addEventListener("click", () => {
    const show = frame.hidden !== false;
    frame.hidden = !show;
    editor.classList.toggle("previewing", show);
    button.textContent = show ? "Close preview" : "Preview";
    if (show) void showPreview(frame, doc);
    else hidePreview();
  });
  const editor = h(
    "div",
    { class: "body-editor" },
    h(
      "div",
      { class: "doc-head" },
      h("strong", {}, "Text"),
      h("span", { class: "spacer" }),
      h("small", { class: "muted" }, "Markdown; the preview shows it in the site's own page, without shortcodes"),
      button,
    ),
    area,
    frame,
  );
  return editor;
}

// ── Bundle files and uploads ───────────────────────────────────────────────────────────────────

export function bundleDir(): string {
  const f = current().entry.files[0];
  return f.path.slice(0, f.path.lastIndexOf("/"));
}

export function filesPanel(): HTMLElement | null {
  const p = current();
  const dir = p.entry.bundle ? bundleDir() : site.media;
  if (!dir) return null;
  const files = p.entry.bundle ? [...p.entry.resources] : [];
  const waiting = p.uploads.map((u) => u.path);
  const allowed = canEdit(`${dir}/upload.jpg`);
  const input = h("input", { type: "file", multiple: true, accept: site.upload_types.map((t) => `.${t}`).join(","), hidden: true });
  input.addEventListener("change", () => {
    if (input.files) void upload(Array.from(input.files), dir);
  });
  return h(
    "section",
    { class: "files" },
    h(
      "div",
      { class: "doc-head" },
      h("strong", {}, p.entry.bundle ? "Files of this page" : "Uploads"),
      h("span", { class: "spacer" }),
      allowed ? h("button", { class: "small", onclick: () => input.click() }, "Upload…") : null,
      input,
    ),
    h("p", { class: "muted" }, p.entry.bundle ? `In ${dir}/` : `Into ${dir}/ (name them as ${site.media_ref ?? `${dir}/`}…)`),
    h(
      "ul",
      { class: "file-list" },
      [...files, ...waiting].map((f) =>
        h(
          "li",
          { class: p.deletes.has(f) ? "deleted" : waiting.includes(f) ? "new" : undefined },
          h("code", {}, f.slice(dir.length + 1)),
          waiting.includes(f) ? h("span", { class: "badge" }, "not saved") : null,
          canEdit(f) && !p.deletes.has(f)
            ? h(
                "button",
                {
                  class: "icon",
                  title: "Delete",
                  onclick: () => {
                    if (waiting.includes(f)) p.uploads = p.uploads.filter((u) => u.path !== f);
                    else p.deletes.add(f);
                    pageView();
                  },
                },
                "×",
              )
            : null,
        ),
      ),
    ),
  );
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
