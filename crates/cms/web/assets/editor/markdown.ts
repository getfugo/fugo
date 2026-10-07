// The text in the rich text editor: its Markdown drawn as the elements of an editable box, and
// the box written back as Markdown. Each block of the text (a paragraph, a list, a table…) is an
// element that knows its block (`data-md-b`), and each inline mark (emphasis, a link, an image…)
// its source (`data-md-i`): what is still as it was drawn is written as it was, byte for byte, so
// that a save changes only what was edited; `tomarkdown.ts` writes the rest anew. Raw HTML,
// shortcodes on lines of their own and link definitions show as their source, in a box where
// they are edited as text: the editor never makes their elements, so no HTML of the text runs.

import { Marked, type Token, type Tokens, type TokensList } from "marked";
import { blockMarkdown, blocksMarkdown, type Context, isBlock, type Style } from "./tomarkdown";

/** The text an editable box shows, as drawn. */
export interface Drawn {
  /** The Markdown, with `\n` line ends (`crlf`: the text had `\r\n`). */
  source: string;
  crlf: boolean;
  /** Its blocks (marked's tokens of the top level), and where each starts in `source`. */
  blocks: { token: Token; start: number }[];
  /** Each block's element as drawn (`outerHTML`; "" for the space between blocks). */
  prints: string[];
  cx: Context;
}

/** The URL that shows the image a text names `src`. */
export type ImageUrl = (src: string) => string;

/** Text as HTML (in an element or an attribute). */
export const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] ?? c);

/** A paragraph of nothing but shortcodes, a line each. */
const SHORTCODES = /^\s*(?:(?:\{\{<[\s\S]*?>\}\}|\{\{%[\s\S]*?%\}\})\s*)+$/;

/** Source the editor shows as such (`kind`: what it is), editable as text. */
function sourceBox(kind: string, text: string, editable: boolean): string {
  const edit = editable ? "plaintext-only" : "false";
  return `<div class="md-raw" data-md-raw="${kind}" contenteditable="false"><div data-md-source contenteditable="${edit}">${esc(text)}</div></div>`;
}

/** marked, drawing for the editor: inline marks with their source in `inline`, images through
 * `url`, HTML as its source. */
function drawer(inline: Context["inline"], url: ImageUrl, editable: boolean): Marked {
  const mark = (raw: string) => {
    inline.push({ raw, print: "" });
    return inline.length - 1;
  };
  return new Marked({
    gfm: true,
    renderer: {
      html(t: Tokens.HTML | Tokens.Tag) {
        if (t.block) return sourceBox("HTML", t.text.replace(/\s+$/, ""), editable);
        return `<span class="md-tag" data-md-raw="HTML" contenteditable="false">${esc(t.text)}</span>`;
      },
      strong(t: Tokens.Strong) {
        return `<strong data-md-i="${mark(t.raw)}">${this.parser.parseInline(t.tokens)}</strong>`;
      },
      em(t: Tokens.Em) {
        return `<em data-md-i="${mark(t.raw)}">${this.parser.parseInline(t.tokens)}</em>`;
      },
      del(t: Tokens.Del) {
        return `<del data-md-i="${mark(t.raw)}">${this.parser.parseInline(t.tokens)}</del>`;
      },
      codespan(t: Tokens.Codespan) {
        return `<code data-md-i="${mark(t.raw)}">${esc(t.text)}</code>`;
      },
      br(t: Tokens.Br) {
        return `<br data-md-i="${mark(t.raw)}">`;
      },
      link(t: Tokens.Link) {
        const title = t.title ? ` title="${esc(t.title)}"` : "";
        return `<a href="${esc(t.href)}"${title} data-md-i="${mark(t.raw)}">${this.parser.parseInline(t.tokens)}</a>`;
      },
      image(t: Tokens.Image) {
        const alt = t.tokens ? this.parser.parseInline(t.tokens, this.parser.textRenderer) : t.text;
        const title = t.title ? ` title="${esc(t.title)}"` : "";
        return `<img src="${esc(url(t.href))}" data-md-src="${esc(t.href)}" alt="${esc(alt)}"${title} data-md-i="${mark(t.raw)}">`;
      },
      checkbox(t: Tokens.Checkbox) {
        return `<input type="checkbox"${t.checked ? " checked" : ""}> `;
      },
    },
  });
}

/** How the text writes what it writes: its first list's bullet, emphasis, rule and fence. */
function styleOf(md: Marked, tokens: TokensList): Style {
  const style: Partial<Style> = {};
  md.walkTokens(tokens, (t) => {
    if (t.type === "list" && !t.ordered) style.bullet ??= t.raw.trimStart()[0];
    if (t.type === "em" && /^[*_]/.test(t.raw)) style.em ??= t.raw[0];
    if (t.type === "strong" && /^(\*\*|__)/.test(t.raw)) style.strong ??= t.raw.slice(0, 2);
    if (t.type === "hr") style.hr ??= t.raw.trim();
    if (t.type === "code" && t.codeBlockStyle !== "indented" && /^\s*(`{3}|~{3})/.test(t.raw)) style.fence ??= t.raw.trimStart().slice(0, 3);
  });
  return { bullet: "-", em: "*", strong: "**", hr: "---", fence: "```", ...style };
}

/** The one element `html` makes, if it makes one and nothing else. */
function elementOf(html: string): Element | null {
  const t = document.createElement("template");
  t.innerHTML = html;
  const nodes = Array.from(t.content.childNodes).filter((n) => n.nodeType !== 3 || (n.textContent ?? "").trim());
  return nodes.length === 1 && nodes[0].nodeType === 1 ? (nodes[0] as Element) : null;
}

/** An empty paragraph, to type in. */
export function emptyParagraph(): HTMLElement {
  const p = document.createElement("p");
  p.append(document.createElement("br"));
  return p;
}

/** Draws `markdown` into `root` (whose elements it replaces), editable or not. */
export function drawMarkdown(root: HTMLElement, markdown: string, url: ImageUrl, editable: boolean): Drawn {
  const crlf = markdown.includes("\r\n");
  const source = markdown.replace(/\r\n?/g, "\n");
  const inline: Context["inline"] = [];
  const md = drawer(inline, url, editable);
  const tokens = md.lexer(source);
  const blocks: Drawn["blocks"] = [];
  let at = 0;
  for (const token of tokens) {
    blocks.push({ token, start: at });
    at += token.raw.length;
  }
  const cx: Context = { style: styleOf(md, tokens), defs: new Set(Object.keys(tokens.links).map((k) => k.toLowerCase())), inline };
  root.replaceChildren();
  // The blocks always add up to the text; if they ever did not, the text shows as one source.
  if (at !== source.length) blocks.splice(0, blocks.length, { token: { type: "html", raw: source, block: true, pre: false, text: source }, start: 0 });
  blocks.forEach(({ token }, i) => {
    if (token.type === "space") return;
    const kind = token.type === "html" ? "HTML" : token.type === "def" ? "Link" : token.type === "paragraph" && SHORTCODES.test(token.raw) ? "Shortcode" : "";
    const raw = token.raw.replace(/\s+$/, "");
    const el = elementOf(kind ? sourceBox(kind, raw, editable) : md.parser([token])) ?? (elementOf(sourceBox("Markdown", raw, editable)) as Element);
    el.setAttribute("data-md-b", String(i));
    root.append(el);
  });
  // Somewhere to type after the last block.
  if (root.lastElementChild?.tagName !== "P") root.append(emptyParagraph());
  for (const el of Array.from(root.querySelectorAll("[data-md-i]"))) inline[Number(el.getAttribute("data-md-i"))].print = el.outerHTML;
  const prints = blocks.map(() => "");
  for (const el of Array.from(root.children)) {
    const b = el.getAttribute("data-md-b");
    if (b !== null) prints[Number(b)] = el.outerHTML;
  }
  return { source, crlf, blocks, prints, cx };
}

/**
 * The Markdown of `root`, drawn as `d`: the blocks still as drawn as they were, the others
 * written anew. Between two blocks that were next to each other goes the text that was between
 * them, if both are as drawn or it holds a blank line (more blank lines, say), else a blank line;
 * the text before the first block and after the last stays where they are the text's first and
 * last.
 */
export function writeMarkdown(root: HTMLElement, d: Drawn): string {
  /** A block of the Markdown: `block`, the one it is as drawn; `from`, the one it was drawn as. */
  const pieces: { block: number | null; from: number | null; text: string }[] = [];
  const used = new Set<number>();
  const placed = new Set<number>();
  let run: Node[] = [];
  const flush = () => {
    const text = run.length ? blocksMarkdown(run, d.cx) : "";
    if (text) pieces.push({ block: null, from: null, text });
    run = [];
  };
  for (const n of Array.from(root.childNodes)) {
    if (!isBlock(n)) {
      run.push(n);
      continue;
    }
    flush();
    const el = n as Element;
    const b = el.getAttribute("data-md-b");
    const i = b === null ? -1 : Number(b);
    // Editing copies an element's attributes to the one it splits off: one of them is the block.
    if (i >= 0 && !used.has(i) && el.outerHTML === d.prints[i]) {
      used.add(i);
      placed.add(i);
      pieces.push({ block: i, from: i, text: d.blocks[i].token.raw.replace(/\s+$/, "") });
      continue;
    }
    const text = blockMarkdown(el, d.cx);
    if (!text) continue;
    pieces.push({ block: null, from: i >= 0 && !placed.has(i) ? i : null, text });
    if (i >= 0) placed.add(i);
  }
  flush();
  const next = (i: number) => d.blocks.findIndex((b, j) => j > i && b.token.type !== "space");
  const end = (i: number) => d.blocks[i].start + d.blocks[i].token.raw.replace(/\s+$/, "").length;
  let out = "";
  pieces.forEach((p, k) => {
    const prev = pieces[k - 1];
    if (!prev) out += p.from !== null && p.from === next(-1) ? d.source.slice(0, d.blocks[p.from].start) : "";
    else {
      const between = prev.from !== null && p.from !== null && next(prev.from) === p.from ? d.source.slice(end(prev.from), d.blocks[p.from].start) : null;
      const keep = between !== null && ((prev.block !== null && p.block !== null) || /\n[ \t]*\n/.test(between));
      out += keep ? between : "\n\n";
    }
    out += p.text;
  });
  const last = pieces.at(-1);
  if (!last) out = d.source.trim() ? "" : d.source;
  else out += last.from !== null && next(last.from) === -1 ? d.source.slice(end(last.from)) : "\n";
  return d.crlf ? out.replace(/\n/g, "\r\n") : out;
}

/** Pasted HTML as the editor draws it: its Markdown (what the text can hold) drawn anew. */
export function pastedHtml(html: string, url: ImageUrl, cx: Context): string {
  const body = new DOMParser().parseFromString(html, "text/html").body;
  for (const el of Array.from(body.querySelectorAll("script, style, template, meta, link, title"))) el.remove();
  // A word processor's list items are paragraphs: a list of them would be a loose list.
  for (const p of Array.from(body.querySelectorAll("li > p:only-child"))) p.replaceWith(...Array.from(p.childNodes));
  const markdown = blocksMarkdown(Array.from(body.childNodes), { ...cx, inline: [] });
  const box = document.createElement("div");
  drawMarkdown(box, markdown, url, true);
  for (const el of Array.from(box.querySelectorAll("[data-md-b], [data-md-i]"))) {
    el.removeAttribute("data-md-b");
    el.removeAttribute("data-md-i");
  }
  // The paragraph to type in that a drawing ends with.
  if (box.lastElementChild?.tagName === "P" && box.lastElementChild.innerHTML === "<br>") box.lastElementChild.remove();
  return box.innerHTML;
}
