// The preview of a page inside the site's own page: the published page of its language (or of
// another page in its folder), with the edited text and front matter (its images too) in place,
// as you type. The site's build marks the element of each page's text (`data-cms-body`, see
// crates/cms/src/body.rs); the text goes there.

import { marked } from "marked";
import { type FrontMatter } from "../codec";
import { type Doc, type Entry } from "./data";
import { imageUrl, isImage } from "./images";
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

/** The comments a text that shares its element is put between (`data-cms-body="1..3"`). */
const START = "cms-body";
const END = "/cms-body";

/**
 * Marks the element of `d` that holds the page's text `data-cms-preview="body"`: the one the
 * site's build (or its layout) marked `data-cms-body`, else the innermost one whose text starts
 * and ends like `text` (the text as published). A mark `data-cms-body="1..3"` says the text is
 * the element's children 1 and 2 (elements only): they go between two comments, which
 * `refresh` puts the text between. False when no element holds the text.
 */
function markBody(d: Document, text: string): boolean {
  const mark = d.querySelector("[data-cms-body]");
  const body = mark && fits(d, mark) ? mark : text.length < 20 ? null : guessBody(d, text);
  body?.setAttribute("data-cms-preview", "body");
  return body !== null;
}

/** Puts the comments around the children of `el` its mark names, if it names some: false when
 * `el` does not have them (a page changed since it was built). */
function fits(d: Document, el: Element): boolean {
  const range = /^(\d+)\.\.(\d+)$/.exec(el.getAttribute("data-cms-body") ?? "");
  if (!range) return true;
  const [from, to] = [Number(range[1]), Number(range[2])];
  const kids = Array.from(el.children);
  if (from >= to || to > kids.length) return false;
  kids[from].before(d.createComment(START));
  kids[to - 1].after(d.createComment(END));
  return true;
}

/** The comments of `body` that the text goes between, when it has them. */
function marksOf(body: Element): [Node, Node] | null {
  const comments = Array.from(body.childNodes).filter((n) => n.nodeType === 8);
  const start = comments.find((n) => n.textContent === START);
  const end = comments.find((n) => n.textContent === END);
  return start && end ? [start, end] : null;
}

/** Whether `node` is in the page's text: between the comments of `body`, else in `body`. */
function inText(body: Element, node: Node): boolean {
  const marks = marksOf(body);
  if (!marks) return body.contains(node);
  // Node.DOCUMENT_POSITION_FOLLOWING (4) and PRECEDING (2).
  return (marks[0].compareDocumentPosition(node) & 4) !== 0 && (marks[1].compareDocumentPosition(node) & 2) !== 0;
}

/** The innermost element of `d` whose text starts and ends like `text`. */
function guessBody(d: Document, text: string): Element | null {
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

/** The name of the image file at `src` without its extension, and without the suffix of a copy
 * the site made of it (`photo_hu_1a2b3c4d5e6f7a8b.webp` is `photo`). */
const stemOf = (src: string) =>
  decodeURIComponent(src.split(/[?#]/)[0].split("/").pop() ?? "")
    .replace(/\.[^.]*$/, "")
    .replace(/_hu_?[0-9a-f]{8,}.*$/, "");

/** In place of the frames of other sites (a video), which the editor's policy does not load:
 * a box that says what they show. */
function embedsShown(d: Document, base: string): void {
  for (const frame of Array.from(d.querySelectorAll("iframe[src]"))) {
    let from: URL;
    try {
      from = new URL(frame.getAttribute("src") ?? "", base);
    } catch {
      continue;
    }
    if (from.origin === location.origin) continue;
    const box = d.createElement("div");
    box.setAttribute("style", "display:grid;place-items:center;aspect-ratio:16/9;max-width:100%;padding:1rem;box-sizing:border-box;background:#eee;color:#555;border-radius:8px;font:14px/1.4 system-ui,sans-serif;text-align:center");
    box.textContent = `Embedded from ${from.host}: not shown in the preview`;
    frame.replaceWith(box);
  }
}

/**
 * The page at `url` (`html`) prepared to show `doc`: without scripts, with absolute URLs (the
 * editor's policy allows no `<base>`), its text's element and the elements that show `doc`'s
 * front matter (`data-cms-field`, or text equal to a value as loaded; images of a file a value
 * names, `data-cms-image`) marked for `refresh`.
 */
export function prepare(html: string, url: string, doc: Doc): string | null {
  const d = new DOMParser().parseFromString(html, "text/html");
  // The text's element first: the build counted the scripts among its children.
  if (!markBody(d, textOf(doc.parts.body))) return null;
  for (const el of Array.from(d.querySelectorAll("script, link[rel=preload][as=script], link[rel=modulepreload]"))) el.remove();
  const body = d.querySelector("[data-cms-preview=body]") as Element;
  for (const [key, value] of Object.entries(doc.original ?? {})) {
    if (typeof value !== "string" || squash(value).length < 4) continue;
    for (const el of Array.from(d.querySelectorAll("title, body *"))) {
      if (el.children.length === 0 && squash(el.textContent) === squash(value)) el.setAttribute("data-cms-field", key);
    }
    if (!isImage(value)) continue;
    for (const img of Array.from(d.querySelectorAll("img[src]"))) {
      if (!inText(body, img) && stemOf(img.getAttribute("src") ?? "") === stemOf(value)) img.setAttribute("data-cms-image", key);
    }
  }
  const page = new URL(url, location.origin).href;
  absolutize(d, page);
  embedsShown(d, page);
  return `<!doctype html>${d.documentElement.outerHTML}`;
}

/** The text alone, as the editor showed it before pages had URLs. */
function plain(doc: Doc): string {
  return `<!doctype html><meta charset="utf-8"><style>body{font:16px/1.6 system-ui,sans-serif;max-width:46rem;margin:1rem auto;padding:0 1rem;color:#222}img{max-width:100%}pre{overflow:auto;background:#f4f4f4;padding:.5rem}</style><div data-cms-preview="body">${marked.parse(doc.body, { async: false })}</div>`;
}

let shown: { frame: HTMLIFrameElement; doc: Doc; base: string } | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;

/** Shows `doc` (the open page's document in its language) in `frame`, unless it shows it. */
export async function showPreview(frame: HTMLIFrameElement, doc: Doc): Promise<void> {
  if (shown?.frame === frame && shown.doc === doc) return;
  const p = current();
  const url = shellUrl(p.entry, p.lang, [...site.entries, ...pending.values()]);
  // A new page borrows another page's layout: its own files resolve against its own folder.
  const own = p.entry.files.find((f) => f.lang === p.lang)?.url;
  const showing = { frame, doc, base: new URL(own ?? url ?? "/", location.origin).href };
  shown = showing;
  const html = url ? await fetchShell(url) : null;
  // Closed, or another document shown, while the page loaded.
  if (shown !== showing) return;
  const page = url && html ? prepare(html, url, doc) : null;
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

/** Puts `html` (the text) into `body`, between its comments when it has them, else in place of
 * what it holds; with the images from `url` and the links made absolute against `base`. */
export function putText(body: Element, html: string, url: (src: string) => string, base: string): void {
  const t = body.ownerDocument.createElement("template");
  t.innerHTML = html;
  for (const img of Array.from(t.content.querySelectorAll("img[src]"))) img.setAttribute("src", url(img.getAttribute("src") ?? ""));
  absolutize(t.content, base);
  const marks = marksOf(body);
  if (!marks) {
    body.replaceChildren(t.content);
    return;
  }
  const [start, end] = marks;
  while (start.nextSibling && start.nextSibling !== end) start.nextSibling.remove();
  end.parentNode?.insertBefore(t.content, end);
}

/** Puts the document's text and front matter values into the shown preview. */
function refresh(): void {
  const d = shown?.frame.contentDocument;
  if (!shown || !d) return;
  const { doc, base } = shown;
  // Images from where the site publishes the page's files, or the browser's copy of those not
  // saved yet.
  const url = imageUrl();
  const body = d.querySelector("[data-cms-preview=body]");
  if (body) putText(body, marked.parse(doc.body, { async: false }), url, base);
  const data: FrontMatter = doc.data ?? {};
  for (const el of Array.from(d.querySelectorAll("[data-cms-field]"))) {
    const value = data[el.getAttribute("data-cms-field") ?? ""];
    if (typeof value === "string" || typeof value === "number") el.textContent = String(value);
  }
  // A file chosen in place of the one the page was published with: the file itself, not the
  // copies the site made of the other (`srcset`, of the image and of its `<picture>`'s sources),
  // until the first is chosen again.
  for (const img of Array.from(d.querySelectorAll("img[data-cms-image]"))) {
    const key = img.getAttribute("data-cms-image") ?? "";
    const value = data[key];
    const sets = [img, ...Array.from(img.closest("picture")?.querySelectorAll("source") ?? [])];
    if (!img.hasAttribute("data-cms-src")) {
      if (value === doc.original?.[key]) continue;
      img.setAttribute("data-cms-src", img.getAttribute("src") ?? "");
      for (const el of sets) el.setAttribute("data-cms-srcset", el.getAttribute("srcset") ?? "");
    }
    const changed = value !== doc.original?.[key] && typeof value === "string" && value !== "";
    img.setAttribute("src", changed ? url(value) : (img.getAttribute("data-cms-src") ?? ""));
    for (const el of sets) {
      const srcset = changed ? "" : (el.getAttribute("data-cms-srcset") ?? "");
      if (srcset) el.setAttribute("srcset", srcset);
      else el.removeAttribute("srcset");
    }
  }
}
