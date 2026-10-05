// The preview of a page inside the site's own page: the published page of its language (or of
// another page in its folder), with the edited text and front matter in place, as you type.

import { marked } from "marked";
import { type FrontMatter } from "../codec";
import { type Doc, type Entry } from "./data";
import { current, pending, site } from "./state";

/**
 * The URL of a published page whose layout the preview of `entry` in `lang` borrows: its own,
 * else that of a page of the same kind in its folder, else in its section. Null when there is
 * none (a site never built, a language without pages).
 */
export function shellUrl(entry: Pick<Entry, "key" | "kind" | "section" | "files">, lang: string, entries: Pick<Entry, "key" | "kind" | "section" | "files">[]): string | null {
  const urlOf = (e: Pick<Entry, "files">) => e.files.find((f) => f.lang === lang)?.url ?? null;
  const own = urlOf(entry);
  if (own) return own;
  const dir = entry.key.includes("/") ? entry.key.slice(0, entry.key.lastIndexOf("/")) : "";
  const others = entries.filter((e) => e.key !== entry.key && e.kind === entry.kind && urlOf(e));
  const inFolder = others.find((e) => (dir ? e.key.startsWith(`${dir}/`) && !e.key.slice(dir.length + 1).includes("/") : !e.key.includes("/")));
  const near = inFolder ?? others.find((e) => e.section === entry.section);
  return near ? urlOf(near) : null;
}

const shells = new Map<string, Promise<string | null>>();

/** The page at `url` (below the site's root) as the site serves it, from the editor's origin. */
function fetchShell(url: string): Promise<string | null> {
  const path = new URL(site.site_url).pathname.replace(/\/$/, "") + url;
  let page = shells.get(path);
  if (!page) {
    page = fetch(path, { credentials: "same-origin" })
      .then((r) => (r.ok ? r.text() : null))
      .catch(() => null);
    shells.set(path, page);
  }
  return page;
}

const squash = (s: string | null | undefined) => (s ?? "").replace(/\s+/g, " ").trim();

/** The text of Markdown, as a reader sees it. */
function textOf(markdown: string): string {
  return squash(new DOMParser().parseFromString(marked.parse(markdown, { async: false }), "text/html").body.textContent);
}

/**
 * The element of `d` that holds the page's text: the one marked `data-cms-body`, else the
 * innermost one whose text starts and ends like `text` (the text as published).
 */
function bodyElement(d: Document, text: string): Element | null {
  const marked = d.querySelector("[data-cms-body]");
  if (marked || text.length < 20) return marked;
  const [start, end] = [text.slice(0, 40), text.slice(-40)];
  let found: Element | null = null;
  for (const el of Array.from(d.body.querySelectorAll("*"))) {
    const t = squash(el.textContent);
    if (t.includes(start) && t.includes(end)) found = el;
  }
  return found;
}

/** Makes the relative URLs of `root`'s links, images and stylesheets absolute against `base`. */
function absolutize(root: ParentNode, base: string): void {
  for (const [selector, attr] of [["[src]", "src"], ["[href]", "href"], ["[srcset]", "srcset"]] as const) {
    for (const el of Array.from(root.querySelectorAll(selector))) {
      const v = el.getAttribute(attr) ?? "";
      if (attr === "srcset") {
        el.setAttribute(attr, v.split(",").map((part) => part.trim().replace(/^\S+/, (u) => new URL(u, base).href)).join(", "));
      } else if (v && !/^(#|[a-z][a-z0-9+.-]*:)/i.test(v)) {
        el.setAttribute(attr, new URL(v, base).href);
      }
    }
  }
}

/**
 * The page at `url` (`html`) prepared to show `doc`: without scripts, with absolute URLs (the
 * editor's policy allows no `<base>`), its text's element and the elements that show `doc`'s
 * front matter (`data-cms-field`, or text equal to a value as loaded) marked for `refresh`.
 */
function prepare(html: string, url: string, doc: Doc): string | null {
  const d = new DOMParser().parseFromString(html, "text/html");
  for (const el of Array.from(d.querySelectorAll("script, link[rel=preload][as=script], link[rel=modulepreload]"))) el.remove();
  const body = bodyElement(d, textOf(doc.parts.body));
  if (!body) return null;
  body.setAttribute("data-cms-preview", "body");
  for (const [key, value] of Object.entries(doc.original ?? {})) {
    if (typeof value !== "string" || squash(value).length < 4) continue;
    for (const el of Array.from(d.querySelectorAll("title, body *"))) {
      if (el.children.length === 0 && squash(el.textContent) === squash(value)) el.setAttribute("data-cms-field", key);
    }
  }
  absolutize(d, new URL(url, location.origin).href);
  return `<!doctype html>${d.documentElement.outerHTML}`;
}

/** The text alone, as the editor showed it before pages had URLs. */
function plain(doc: Doc): string {
  return `<!doctype html><meta charset="utf-8"><style>body{font:16px/1.6 system-ui,sans-serif;max-width:46rem;margin:1rem auto;padding:0 1rem;color:#222}img{max-width:100%}pre{overflow:auto;background:#f4f4f4;padding:.5rem}</style><div data-cms-preview="body">${marked.parse(doc.body, { async: false })}</div>`;
}

let shown: { frame: HTMLIFrameElement; doc: Doc; base: string } | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;

/** Shows `doc` (the open page's document in its language) in `frame`. */
export async function showPreview(frame: HTMLIFrameElement, doc: Doc): Promise<void> {
  const p = current();
  const url = shellUrl(p.entry, p.lang, [...site.entries, ...pending.values()]);
  const html = url ? await fetchShell(url) : null;
  const page = url && html ? prepare(html, url, doc) : null;
  // A new page borrows another page's layout: its own files resolve against its own folder.
  const own = p.entry.files.find((f) => f.lang === p.lang)?.url;
  shown = { frame, doc, base: new URL(own ?? url ?? "/", location.origin).href };
  frame.onload = () => refresh();
  frame.srcdoc = page ?? plain(doc);
}

export function hidePreview(): void {
  shown = null;
}

/** Updates the shown preview soon (after a burst of typing). */
export function schedulePreview(): void {
  if (!shown) return;
  clearTimeout(timer);
  timer = setTimeout(refresh, 150);
}

/** Puts the document's text and front matter values into the shown preview. */
function refresh(): void {
  const d = shown?.frame.contentDocument;
  if (!shown || !d) return;
  const { doc, base } = shown;
  const body = d.querySelector("[data-cms-preview=body]");
  if (body) {
    body.innerHTML = marked.parse(doc.body, { async: false });
    absolutize(body, base);
    // Files uploaded but not saved yet show from the browser's copy.
    for (const img of Array.from(body.querySelectorAll("img[src]"))) {
      const upload = current().uploads.find((u) => img.getAttribute("src")?.endsWith(`/${u.path.split("/").pop()}`));
      if (upload) img.setAttribute("src", `data:${mimeOf(upload.path)};base64,${upload.content}`);
    }
  }
  const data: FrontMatter = doc.data ?? {};
  for (const el of Array.from(d.querySelectorAll("[data-cms-field]"))) {
    const value = data[el.getAttribute("data-cms-field") ?? ""];
    if (typeof value === "string" || typeof value === "number") el.textContent = String(value);
  }
}

function mimeOf(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return { jpg: "image/jpeg", jpeg: "image/jpeg", png: "image/png", gif: "image/gif", webp: "image/webp", svg: "image/svg+xml", avif: "image/avif" }[ext] ?? "application/octet-stream";
}
