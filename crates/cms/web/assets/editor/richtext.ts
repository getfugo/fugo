// The rich text editor of a page's text: a toolbar, and an editable box that shows the Markdown
// as it reads (`markdown.ts` draws it and writes it back on every change). Its edits are the
// browser's editing commands, so that the browser's undo undoes them; but for two that the
// browser gets wrong (code, and a quote back to paragraphs), which change the elements.

import { html, nothing, type TemplateResult } from "lit-html";
import { ref } from "lit-html/directives/ref.js";
import { type Doc } from "./data";
import { imageChoices } from "./inputs";
import { type Drawn, drawMarkdown, emptyParagraph, esc, type ImageUrl, pastedHtml, writeMarkdown } from "./markdown";
import { pageView } from "./pages";
import { mimeOf } from "./preview";
import { current, site } from "./state";
import { isBlock } from "./tomarkdown";

/** The boxes drawn: the document each shows, as drawn, and its text since. */
const boxes = new WeakMap<Element, { doc: Doc; drawn: Drawn; body: string; editable: boolean }>();

/** The document whose toolbar shows the form for an image. */
let imaging: Doc | null = null;

/** The last selection in a box, for the commands of the toolbar's inputs (which take the focus). */
let saved: Range | null = null;

const BLOCK_TYPES: [string, string][] = [
  ["p", "Paragraph"],
  ["h1", "Heading 1"],
  ["h2", "Heading 2"],
  ["h3", "Heading 3"],
  ["h4", "Heading 4"],
  ["blockquote", "Quote"],
  ["pre", "Code block"],
];

export function richEditor(doc: Doc, readOnly: boolean): TemplateResult {
  setUp();
  const mount = (el?: Element) => {
    if (el) draw(el as HTMLElement, doc, !readOnly);
  };
  return html`
    <div class="rich">
      ${readOnly ? nothing : toolbar(doc)}
      <div
        class="rich-box"
        ${ref(mount)}
        contenteditable=${readOnly ? "false" : "true"}
        role="textbox"
        aria-multiline="true"
        aria-label="Text"
        @input=${input}
        @keydown=${keydown}
        @paste=${paste}
        @click=${click}
      ></div>
    </div>
  `;
}

/** Draws `doc`'s text into `box`, unless the box shows it as it is. */
function draw(box: HTMLElement, doc: Doc, editable: boolean): void {
  const shown = boxes.get(box);
  if (shown && shown.doc === doc && shown.body === doc.body && shown.editable === editable) return;
  boxes.set(box, { doc, drawn: drawMarkdown(box, doc.body, imageUrl(), editable), body: doc.body, editable });
}

/** The box a toolbar's control is for. */
const boxOf = (el: Element) => el.closest(".rich")?.querySelector<HTMLElement>(".rich-box") ?? null;

/** Writes the box's text into its document (on every change of it). */
function input(ev: Event): void {
  const box = ev.currentTarget as HTMLElement;
  const shown = boxes.get(box);
  if (!shown) return;
  // Somewhere to type after a list, a table or a code block at the end.
  if (box.lastElementChild?.tagName !== "P") box.append(emptyParagraph());
  shown.doc.body = shown.body = writeMarkdown(box, shown.drawn);
}

/** The URL of the images of the open page's text: its own uploads, else as the site serves
 * them, relative to the page. */
function imageUrl(): ImageUrl {
  const p = current();
  const own = p.entry.files.find((f) => f.lang === p.lang)?.url ?? "/";
  const base = new URL(new URL(site.site_url).pathname.replace(/\/$/, "") + own, location.origin).href;
  return (src) => {
    const name = src.split("/").pop();
    const upload = p.uploads.find((u) => u.path.split("/").pop() === name);
    if (upload) return `data:${mimeOf(upload.path)};base64,${upload.content}`;
    try {
      return new URL(src, base).href;
    } catch {
      return src;
    }
  };
}

let ready = false;

/** Once: the browser's editing makes paragraphs and elements (not styles), and the toolbar
 * follows the selection. */
function setUp(): void {
  if (ready) return;
  ready = true;
  command("defaultParagraphSeparator", "p");
  command("styleWithCSS", "false");
  document.addEventListener("selectionchange", () => {
    const sel = document.getSelection();
    const box = sel?.rangeCount ? elementAt(sel.getRangeAt(0).commonAncestorContainer)?.closest(".rich-box") : null;
    if (!sel || !box) return;
    saved = sel.getRangeAt(0).cloneRange();
    showState(box);
  });
}

const elementAt = (n: Node | null | undefined) => (n ? (n.nodeType === 1 ? (n as Element) : n.parentElement) : null);

/** The browser's editing command (none in a browser without them). */
function command(name: string, value?: string): boolean {
  try {
    return typeof document.execCommand === "function" && document.execCommand(name, false, value);
  } catch {
    return false;
  }
}

/** Runs an editing command in `box`: on `at`, else on the selection, else (the selection is in
 * an input of the toolbar) on the box's last selection. */
function exec(box: HTMLElement, name: string, value?: string, at?: Range): void {
  const sel = document.getSelection();
  const inBox = !!sel?.rangeCount && box.contains(sel.getRangeAt(0).commonAncestorContainer);
  box.focus();
  const range = at ?? (inBox ? null : saved && box.contains(saved.commonAncestorContainer) ? saved : null);
  if (sel && range) {
    sel.removeAllRanges();
    sel.addRange(range);
  }
  command(name, value);
}

/** The element matching `selector` that holds the selection in `box`. */
function closestIn(box: Element, selector: string): Element | null {
  const found = elementAt(document.getSelection()?.anchorNode)?.closest(selector);
  return found && box.contains(found) ? found : null;
}

/** The toolbar's buttons pressed, and its block type, as at the selection. */
function showState(box: Element): void {
  const bar = box.parentElement?.querySelector(".toolbar");
  if (!bar) return;
  for (const b of Array.from(bar.querySelectorAll<HTMLElement>("button[data-state]"))) {
    let on = false;
    try {
      on = document.queryCommandState(b.dataset.state ?? "");
    } catch {
      // A browser without the command.
    }
    b.setAttribute("aria-pressed", String(on));
  }
  const code = closestIn(box, "code");
  bar.querySelector("button[data-code]")?.setAttribute("aria-pressed", String(code !== null && !code.closest("pre")));
  const select = bar.querySelector<HTMLSelectElement>("select.block-type");
  if (select) select.value = blockType(box);
}

/** The block type at the selection: a heading's or code block's, a quote's, else a paragraph's. */
function blockType(box: Element): string {
  const at = closestIn(box, "h1, h2, h3, h4, h5, h6, pre, blockquote");
  const tag = at?.tagName.toLowerCase() ?? "p";
  return BLOCK_TYPES.some(([t]) => t === tag) ? tag : "p";
}

function toolbar(doc: Doc): TemplateResult {
  // The buttons keep the focus (and the selection) in the box.
  const keep = (ev: Event) => {
    if ((ev.target as Element).closest("button")) ev.preventDefault();
  };
  const run = (f: (box: HTMLElement) => void) => (ev: Event) => {
    const box = boxOf(ev.currentTarget as Element);
    if (box) f(box);
  };
  const button = (label: string, title: string, f: (box: HTMLElement) => void, state?: string) =>
    html`<button type="button" class="small" title=${title} aria-label=${title} data-state=${state ?? nothing} @click=${run(f)}>${label}</button>`;
  const changeType = (ev: Event) => {
    const select = ev.target as HTMLSelectElement;
    const box = boxOf(select);
    if (box) setBlockType(box, select.value);
  };
  const imageForm = () => {
    imaging = imaging === doc ? null : doc;
    pageView();
  };
  return html`
    <div class="toolbar" role="toolbar" aria-label="Formatting" @mousedown=${keep}>
      <select class="block-type" aria-label="Paragraph style" @change=${changeType}>
        ${BLOCK_TYPES.map(([tag, label]) => html`<option value=${tag}>${label}</option>`)}
      </select>
      ${button("B", "Bold", (box) => exec(box, "bold"), "bold")}
      ${button("I", "Italic", (box) => exec(box, "italic"), "italic")}
      ${button("S", "Strikethrough", (box) => exec(box, "strikeThrough"), "strikeThrough")}
      <button type="button" class="small" title="Code" aria-label="Code" data-code @click=${run(toggleCode)}>&lt;/&gt;</button>
      ${button("Link", "Link (Ctrl+K)", link)}
      ${button("• List", "Bulleted list", (box) => exec(box, "insertUnorderedList"), "insertUnorderedList")}
      ${button("1. List", "Numbered list", (box) => exec(box, "insertOrderedList"), "insertOrderedList")}
      <button type="button" class="small" title="Image" aria-label="Image" aria-expanded=${imaging === doc} @click=${imageForm}>Image</button>
      ${button("―", "Horizontal rule", (box) => exec(box, "insertHorizontalRule"))}
    </div>
    ${imaging === doc ? imagePanel() : nothing}
  `;
}

function setBlockType(box: HTMLElement, tag: string): void {
  const quote = closestIn(box, "blockquote");
  if (tag === "blockquote") {
    if (!quote) exec(box, "formatBlock", "<blockquote>");
  } else if (quote && tag === "p" && !closestIn(box, "h1, h2, h3, h4, h5, h6, pre")) unquote(box, quote);
  else exec(box, "formatBlock", `<${tag}>`);
}

/**
 * A quote's blocks in its place, its text in paragraphs. The browser's editing would merge its
 * text into the block after it, so this edits the elements (the browser's undo does not undo it).
 */
function unquote(box: HTMLElement, quote: Element): void {
  const blocks: Node[] = [];
  let run: Node[] = [];
  const flush = () => {
    if (run.some((n) => n.nodeType !== 3 || (n.textContent ?? "").trim())) {
      const p = document.createElement("p");
      p.append(...run);
      blocks.push(p);
    }
    run = [];
  };
  for (const n of Array.from(quote.childNodes)) {
    if (isBlock(n)) {
      flush();
      blocks.push(n);
    } else run.push(n);
  }
  flush();
  quote.replaceWith(...blocks);
  if (blocks[0]) caretIn(blocks[0]);
  box.dispatchEvent(new Event("input", { bubbles: true }));
}

/** The caret at the end of `node`'s text. */
function caretIn(node: Node): void {
  const range = document.createRange();
  range.selectNodeContents(node);
  range.collapse(false);
  const sel = document.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
}

/** The block element that holds `n`. */
const blockAt = (n: Node) => elementAt(n)?.closest("p, li, h1, h2, h3, h4, h5, h6, td, th, pre, blockquote, .rich-box");

/**
 * Code: the selected text (in one block) as code, or the code at the selection back to text.
 * The browser's editing would make `<code>` a styled `<span>`, so this edits the elements.
 */
function toggleCode(box: HTMLElement): void {
  const sel = document.getSelection();
  const range = sel?.rangeCount ? sel.getRangeAt(0) : null;
  if (!range || !box.contains(range.commonAncestorContainer)) return;
  const code = closestIn(box, "code");
  if (code && !code.closest("pre")) {
    const text = document.createTextNode(code.textContent ?? "");
    code.replaceWith(text);
    caretIn(text);
  } else {
    const text = range.toString();
    if (!text || blockAt(range.startContainer) !== blockAt(range.endContainer) || elementAt(range.commonAncestorContainer)?.closest("pre")) return;
    const el = document.createElement("code");
    el.textContent = text;
    range.deleteContents();
    range.insertNode(el);
    caretIn(el);
  }
  box.dispatchEvent(new Event("input", { bubbles: true }));
}

/** A link on the selected text, or the link at the selection changed or removed. */
function link(box: HTMLElement): void {
  const a = closestIn(box, "a");
  const sel = document.getSelection();
  // The prompt takes the focus, and the selection with it.
  const at = sel?.rangeCount ? sel.getRangeAt(0).cloneRange() : undefined;
  const url = prompt("Link to (empty for none):", a?.getAttribute("href") ?? "https://");
  if (url === null) return;
  const href = url.trim();
  if (a) {
    if (href) {
      a.setAttribute("href", href);
      box.dispatchEvent(new Event("input", { bubbles: true }));
    } else {
      const range = document.createRange();
      range.selectNodeContents(a);
      exec(box, "unlink", undefined, range);
    }
    return;
  }
  if (!href) return;
  if (at?.collapsed ?? true) exec(box, "insertHTML", `<a href="${esc(href)}">${esc(href)}</a>`, at);
  else exec(box, "createLink", href, at);
}

/** The form for an image: one of the page's files (or any address) and its description. */
function imagePanel(): TemplateResult {
  const choices = imageChoices();
  const insert = (ev: Event) => {
    ev.preventDefault();
    const form = ev.currentTarget as HTMLFormElement;
    const data = new FormData(form);
    const src = String(data.get("src") ?? "").trim();
    const alt = String(data.get("alt") ?? "").trim();
    const box = boxOf(form);
    if (!src || !box) return;
    exec(box, "insertHTML", `<img src="${esc(imageUrl()(src))}" data-md-src="${esc(src)}" alt="${esc(alt)}">`);
    imaging = null;
    pageView();
  };
  const close = () => {
    imaging = null;
    pageView();
  };
  return html`
    <form class="image-form" @submit=${insert}>
      <input name="src" required list="rich-images" placeholder=${choices[0] ?? "photo.jpg"} aria-label="Image file" />
      <datalist id="rich-images">${choices.map((c) => html`<option value=${c}></option>`)}</datalist>
      <input name="alt" placeholder="Description" aria-label="Description" />
      <button type="submit" class="small primary">Insert</button>
      <button type="button" class="small" @click=${close}>Cancel</button>
      ${choices.length ? nothing : html`<small class="muted">Upload images under the page's files to choose them here.</small>`}
    </form>
  `;
}

function keydown(ev: KeyboardEvent): void {
  const box = ev.currentTarget as HTMLElement;
  // The source of HTML or a shortcode is text.
  if ((ev.target as Element).closest?.("[data-md-source]")) return;
  if (ev.key === "Tab" && closestIn(box, "li")) {
    ev.preventDefault();
    exec(box, ev.shiftKey ? "outdent" : "indent");
  } else if (ev.key === "Enter" && closestIn(box, "pre")) {
    ev.preventDefault();
    exec(box, "insertLineBreak");
  } else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "k") {
    ev.preventDefault();
    link(box);
  }
}

/** Pasted text: HTML as what Markdown can hold of it, and text as text. */
function paste(ev: ClipboardEvent): void {
  const box = ev.currentTarget as HTMLElement;
  const data = ev.clipboardData;
  const shown = boxes.get(box);
  // Into source, the browser pastes text.
  if (!data || !shown || (ev.target as Element).closest?.("[data-md-source]") || closestIn(box, "[data-md-source]")) return;
  ev.preventDefault();
  const htmlText = data.getData("text/html");
  if (htmlText && !closestIn(box, "pre")) exec(box, "insertHTML", pastedHtml(htmlText, imageUrl(), shown.drawn.cx));
  else exec(box, "insertText", data.getData("text/plain"));
}

/** A task's checkbox: ticked or not, in the text too. */
function click(ev: MouseEvent): void {
  const box = ev.currentTarget as HTMLElement;
  const t = ev.target as HTMLInputElement;
  if (t.tagName !== "INPUT" || t.type !== "checkbox" || box.getAttribute("contenteditable") !== "true") return;
  const on = !t.hasAttribute("checked");
  // The browser's own change of the box is undone; the attribute says what the text has.
  ev.preventDefault();
  t.toggleAttribute("checked", on);
  setTimeout(() => (t.checked = on));
  box.dispatchEvent(new Event("input", { bubbles: true }));
}
