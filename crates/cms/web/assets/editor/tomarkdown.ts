// Elements written as Markdown: a block of the rich text editor that was edited (or pasted), with
// its inline marks, links and images. Text gets a backslash only where a character would
// otherwise start Markdown of its own; shortcodes (`{{< … >}}`, `{{% … %}}`) are left as they are.
// An element that `markdown.ts` drew and that did not change is written as its source was.

/** How the text writes what Markdown can write in more than one way, as most of it does. */
export interface Style {
  bullet: string;
  em: string;
  strong: string;
  hr: string;
  fence: string;
}

/** What writing a block needs: the text's style, the labels of its link definitions, and the
 * inline marks as drawn (`data-md-i`: their source, and their element as drawn). */
export interface Context {
  style: Style;
  defs: Set<string>;
  inline: { raw: string; print: string }[];
  /** Writing the text of a link, where brackets would end it. */
  inLink?: boolean;
}

const BLOCKS = new Set([
  "P", "H1", "H2", "H3", "H4", "H5", "H6", "UL", "OL", "LI", "BLOCKQUOTE", "PRE", "HR", "TABLE", "DIV", "SECTION",
  "ARTICLE", "ASIDE", "HEADER", "FOOTER", "MAIN", "NAV", "FIGURE", "FIGCAPTION", "DL", "DT", "DD", "DETAILS", "SUMMARY",
]);

const isElement = (n: Node): n is Element => n.nodeType === 1;

/** Whether `n` is a block: an element of its own lines, or source the editor shows as a block. */
export const isBlock = (n: Node) => isElement(n) && BLOCKS.has(n.tagName);

const BLOCK_SELECTOR = Array.from(BLOCKS, (t) => t.toLowerCase()).join(",");

/** An inline element around blocks (pasted text: Google Docs wraps a copy in a `<b>`). */
const holdsBlocks = (n: Node) => isElement(n) && !isBlock(n) && n.querySelector(BLOCK_SELECTOR) !== null;

/** The text of an element that holds source (a code block, raw HTML, a shortcode): its line
 * breaks and lines (`<br>`, `<div>`, as editing makes them) as new lines. */
export function sourceOf(el: Node): string {
  let out = "";
  const walk = (n: Node) => {
    for (const c of Array.from(n.childNodes)) {
      if (c.nodeType === 3) out += (c as Text).data;
      else if (c.nodeName === "BR") out += "\n";
      else if (isElement(c)) {
        if ((c.tagName === "DIV" || c.tagName === "P") && out && !out.endsWith("\n")) out += "\n";
        walk(c);
      }
    }
  };
  walk(el);
  return out.replace(/\u00a0/g, " ").replace(/\n+$/, "");
}

/** Blocks, as Markdown: the runs of inline nodes between them as paragraphs, the blocks joined
 * by `sep` (a blank line, or one line break inside a tight list item). */
export function blocksMarkdown(nodes: Node[], cx: Context, sep = "\n\n"): string {
  const out: string[] = [];
  let run: Node[] = [];
  const flush = () => {
    const text = paragraph(run, cx);
    if (text) out.push(text);
    run = [];
  };
  for (const n of nodes) {
    if (!isBlock(n) && !holdsBlocks(n)) {
      run.push(n);
      continue;
    }
    flush();
    const text = isBlock(n) ? blockMarkdown(n as Element, cx) : blocksMarkdown(Array.from(n.childNodes), cx, sep);
    if (text) out.push(text);
  }
  flush();
  return out.join(sep);
}

/** One block element as Markdown ("" when it holds nothing). */
export function blockMarkdown(el: Element, cx: Context): string {
  if (el.hasAttribute("data-md-raw")) return sourceOf(el.querySelector("[data-md-source]") ?? el).trim();
  const tag = el.tagName;
  // A paragraph or a heading around blocks (editing puts a new list in the paragraph it was).
  if ((tag === "P" || /^H[1-6]$/.test(tag)) && el.querySelector(BLOCK_SELECTOR)) return blocksMarkdown(Array.from(el.childNodes), cx);
  if (/^H[1-6]$/.test(tag)) {
    const text = paragraph(Array.from(el.childNodes), cx).replace(/\n/g, " ");
    // A closing sequence of `#`s would be cut off.
    return text ? `${"#".repeat(Number(tag[1]))} ${text.replace(/(^|\s)(#+)$/, "$1\\$2")}` : "";
  }
  switch (tag) {
    case "P":
      return paragraph(Array.from(el.childNodes), cx);
    case "UL":
    case "OL":
      return list(el, cx);
    case "BLOCKQUOTE": {
      const inner = blocksMarkdown(Array.from(el.childNodes), cx);
      return inner ? inner.split("\n").map((l) => (l ? `> ${l}` : ">")).join("\n") : "";
    }
    case "PRE":
      return codeBlock(el, cx);
    case "HR":
      return cx.style.hr;
    case "TABLE":
      return table(el, cx);
    default:
      return blocksMarkdown(Array.from(el.childNodes), cx);
  }
}

/** Inline nodes as one paragraph, its lines made safe to start a line. */
function paragraph(nodes: Node[], cx: Context): string {
  return lines(inline(nodes, cx));
}

/** A space of a line break as drawn (`  ` at the end of a line), kept from the trimming of
 * `lines`. */
const BREAK = "\ue000";

/**
 * Inline Markdown made safe line by line: no line starts what would make it a heading, a list
 * item, a quote or a rule, no line keeps the spaces around it (spaces at the end would be a
 * line break, at the start code), and blank lines go (they would end the paragraph).
 */
function lines(text: string): string {
  return text
    .split("\n")
    .map((l) => l.replace(/^[ \t]+/, "").replace(/[ \t]+$/, ""))
    .filter((l) => l !== "")
    .map((l) =>
      l
        .replace(/^(#{1,6}(?:\s|$)|>|[-+*](?:\s|$))/, "\\$1")
        .replace(/^(\d{1,9})([.)])(\s|$)/, "$1\\$2$3")
        .replace(/^([-=_*])(?=(?:\s*\1)+\s*$)/, "\\$1"),
    )
    .join("\n")
    .replaceAll(BREAK, " ");
}

function inline(nodes: Node[], cx: Context): string {
  return nodes.map((n) => (n.nodeType === 3 ? escapeText((n as Text).data, cx) : isElement(n) ? inlineElement(n, cx) : "")).join("");
}

/** The source of an element `markdown.ts` drew, if it is still as drawn. */
function drawn(el: Element, cx: Context): string | null {
  const i = el.getAttribute("data-md-i");
  const known = i === null ? undefined : cx.inline[Number(i)];
  return known && el.outerHTML === known.print ? known.raw : null;
}

function inlineElement(el: Element, cx: Context): string {
  if (el.hasAttribute("data-md-raw")) return el.textContent ?? "";
  const known = drawn(el, cx);
  if (known !== null) return el.tagName === "BR" ? known.replace(/ /g, BREAK) : known;
  const style = (el as HTMLElement).style;
  switch (el.tagName) {
    case "BR":
      return isLast(el) ? "" : "\\\n";
    case "STRONG":
    case "B":
      // What a word processor marks bold to say nothing (Google Docs wraps a copy in one).
      return /^(normal|[1-5]00)$/.test(style?.fontWeight ?? "") ? inline(Array.from(el.childNodes), cx) : mark(el, cx.style.strong, "strong", cx);
    case "EM":
    case "I":
      return mark(el, cx.style.em, "em", cx);
    case "DEL":
    case "S":
    case "STRIKE":
      return mark(el, "~~", "del", cx);
    case "CODE":
    case "TT":
    case "KBD":
    case "SAMP":
      return codeSpan(el.textContent ?? "");
    case "A":
      return link(el, cx);
    case "IMG":
      return image(el);
    case "INPUT":
      return "";
  }
  // Pasted text says its marks with styles.
  if (style && /^(bold|[6-9]00)$/.test(style.fontWeight)) return mark(el, cx.style.strong, "strong", cx);
  if (style?.fontStyle === "italic") return mark(el, cx.style.em, "em", cx);
  if (style?.textDecoration?.includes("line-through")) return mark(el, "~~", "del", cx);
  return inline(Array.from(el.childNodes), cx);
}

/** Whether nothing but white space follows `node` in its block. */
function isLast(node: Node): boolean {
  for (let n: Node | null = node; n && !isBlock(n); n = n.parentNode) {
    for (let s = n.nextSibling; s; s = s.nextSibling) {
      if (s.nodeType !== 3 || (s as Text).data.trim()) return false;
    }
  }
  return true;
}

const MARKS: Record<string, string> = { strong: "STRONG,B", em: "EM,I", del: "DEL,S,STRIKE" };

/** An inline mark: `marker` around its text, the spaces at its ends outside it. */
function mark(el: Element, marker: string, kind: string, cx: Context): string {
  const inner = inline(Array.from(el.childNodes), cx);
  // Inside the same mark (pasted text), or around nothing: no marker.
  const tags = MARKS[kind].split(",");
  for (let p = el.parentElement; p && !isBlock(p); p = p.parentElement) {
    if (tags.includes(p.tagName)) return inner;
  }
  const m = /^(\s*)([\s\S]*?)(\s*)$/.exec(inner);
  if (!m || !m[2]) return inner;
  // `_` cannot mark part of a word.
  const word = /[\p{L}\p{N}]/u;
  const before = el.previousSibling?.textContent?.slice(-1) ?? "";
  const after = el.nextSibling?.textContent?.slice(0, 1) ?? "";
  const use = marker.startsWith("_") && (word.test(before) || word.test(after)) ? marker.replace(/_/g, "*") : marker;
  return `${m[1]}${use}${m[2]}${use}${m[3]}`;
}

function codeSpan(text: string): string {
  const code = text.replace(/\u00a0/g, " ").replace(/\n/g, " ");
  if (!code) return "";
  const longest = Math.max(0, ...Array.from(code.matchAll(/`+/g), (m) => m[0].length));
  const fence = "`".repeat(longest + 1);
  const pad = code.startsWith("`") || code.endsWith("`") || (/^ .* $/.test(code) && code.trim()) ? " " : "";
  return `${fence}${pad}${code}${pad}${fence}`;
}

/** A link's or an image's destination: in `<…>` when it has spaces or brackets. */
function destination(url: string): string {
  return url === "" || /[\s<>()]/.test(url) ? `<${url.replace(/[<>]/g, "\\$&")}>` : url;
}

const titled = (title: string | null) => (title ? ` "${title.replace(/["\\]/g, "\\$&")}"` : "");

function link(el: Element, cx: Context): string {
  const href = el.getAttribute("href") ?? "";
  const text = inline(Array.from(el.childNodes), { ...cx, inLink: true });
  if (!href) return text;
  const title = el.getAttribute("title");
  const shown = el.textContent ?? "";
  if (!title && shown === href && /^[a-z][a-z0-9+.-]*:[^\s<>]*$/i.test(href)) return `<${href}>`;
  return `[${text}](${destination(href)}${titled(title)})`;
}

function image(el: Element): string {
  const src = el.getAttribute("data-md-src") ?? el.getAttribute("src") ?? "";
  if (!src) return "";
  const alt = (el.getAttribute("alt") ?? "").replace(/[[\]\\]/g, "\\$&");
  return `![${alt}](${destination(src)}${titled(el.getAttribute("title"))})`;
}

const SHORTCODES = /\{\{<[\s\S]*?>\}\}|\{\{%[\s\S]*?%\}\}/g;

const word = /[\p{L}\p{N}]/u;
const space = (c: string | undefined) => c === undefined || /\s/.test(c);

/** Text as Markdown: a backslash before what would start Markdown, shortcodes as they are. */
export function escapeText(text: string, cx: Context): string {
  const s = text.replace(/\u00a0/g, " ");
  const shortcodes = new Map(Array.from(s.matchAll(SHORTCODES), (m) => [m.index, m.index + m[0].length]));
  const tildes = (s.match(/~/g) ?? []).length > 1;
  let out = "";
  for (let i = 0; i < s.length; i++) {
    const end = shortcodes.get(i);
    if (end !== undefined) {
      out += s.slice(i, end);
      i = end - 1;
    } else out += escapeChar(s, i, tildes, cx);
  }
  return out;
}

/** The character at `i` of `text`, escaped if it would start Markdown there. */
function escapeChar(text: string, i: number, tildes: boolean, cx: Context): string {
  const c = text[i];
  const [prev, next] = [text[i - 1], text[i + 1]];
  switch (c) {
    case "\\":
      return next === undefined || /[!-/:-@[-`{-~]/.test(next) ? "\\\\" : c;
    case "`":
      return "\\`";
    case "*":
      return space(prev) && space(next) && prev !== undefined && next !== undefined ? c : "\\*";
    case "_":
      return prev !== undefined && next !== undefined && word.test(prev) && word.test(next) ? c : "\\_";
    case "~":
      return tildes ? "\\~" : c;
    case "<":
      return next !== undefined && /[A-Za-z/!?]/.test(next) ? "\\<" : c;
    case "&":
      return /^&(#\d+|#[xX][\da-fA-F]+|[A-Za-z][A-Za-z\d]*);/.test(text.slice(i)) ? "&amp;" : c;
    case "[":
      return cx.inLink || opensLink(text.slice(i), cx) ? "\\[" : c;
    case "]":
      return cx.inLink ? "\\]" : c;
    default:
      return c;
  }
}

/**
 * Whether `[` at the start of `rest` would open a link: `[text](`, `[text][`, `[text]:`, or a
 * `[label]` that a definition names. A footnote (`[^1]`) and a link whose destination is a
 * shortcode (Hugo's `[text]({{< ref … >}})`) are what the author wrote.
 */
function opensLink(rest: string, cx: Context): boolean {
  const m = /^\[([^\]]*)\](.?)(.?.?)/.exec(rest);
  if (!m || m[1].startsWith("^")) return false;
  if (m[2] === "(") return m[3] !== "{{";
  return m[2] === "[" || m[2] === ":" || cx.defs.has(m[1].trim().toLowerCase());
}

/**
 * A list, its items a marker each and their other lines indented below it. Editing makes lists
 * HTML does not: a list right inside another (indenting an item), text between the items; a
 * nested list goes with the item before it, and text is an item of its own.
 */
function list(el: Element, cx: Context): string {
  const ordered = el.tagName === "OL";
  const start = Number(el.getAttribute("start") ?? "1") || 1;
  const loose = Array.from(el.children).some((li) => li.tagName === "LI" && Array.from(li.children).some((c) => c.tagName === "P"));
  const sep = loose ? "\n\n" : "\n";
  const items: { marker: string; body: string }[] = [];
  const add = (body: string) => {
    if (body) items.push({ marker: ordered ? `${start + items.length}.` : cx.style.bullet, body });
  };
  let run: Node[] = [];
  const flush = () => {
    add(run.length ? blocksMarkdown(run, cx, sep) : "");
    run = [];
  };
  for (const child of Array.from(el.childNodes)) {
    const tag = isElement(child) ? child.tagName : "";
    if (tag !== "LI" && tag !== "UL" && tag !== "OL") {
      run.push(child);
      continue;
    }
    flush();
    if (tag === "LI") add(item(child as Element, cx, sep));
    else if (items.length) items[items.length - 1].body += `${sep}${list(child as Element, cx)}`;
    else add(list(child as Element, cx));
  }
  flush();
  return items
    .map(({ marker, body }) => {
      const pad = " ".repeat(marker.length + 1);
      const [first, ...rest] = body.split("\n");
      return [`${marker} ${first}`.trimEnd(), ...rest.map((l) => (l ? pad + l : l))].join("\n");
    })
    .join(sep);
}

/** A list item's text: its task's box (`[x]`), its blocks. */
function item(li: Element, cx: Context, sep: string): string {
  const box = li.querySelector("input[type=checkbox]");
  const task = box && box.closest("li") === li ? (box.hasAttribute("checked") ? "[x] " : "[ ] ") : "";
  const body = blocksMarkdown(Array.from(li.childNodes), cx, sep);
  return body || task ? `${task}${body}` : "";
}

function codeBlock(el: Element, cx: Context): string {
  // All of the block's text: editing may put some beside its `<code>`.
  const text = sourceOf(el);
  const lang = /(?:^|\s)language-(\S+)/.exec(el.querySelector("code")?.getAttribute("class") ?? "")?.[1] ?? "";
  const ch = cx.style.fence[0];
  const runs = Array.from(text.matchAll(new RegExp(`^ {0,3}(\\${ch}+)`, "gm")), (m) => m[1].length);
  const fence = ch.repeat(Math.max(3, ...runs.map((n) => n + 1)));
  return `${fence}${lang}\n${text}${text ? "\n" : ""}${fence}`;
}

function table(el: Element, cx: Context): string {
  const rows = Array.from(el.querySelectorAll("tr")).filter((r) => r.closest("table") === el);
  if (!rows.length) return "";
  const cells = rows.map((r) =>
    Array.from(r.children)
      .filter((c) => c.tagName === "TD" || c.tagName === "TH")
      .map((c) => inline(Array.from(c.childNodes), cx).replace(/\s*\n\s*/g, " ").replace(/\|/g, "\\|").trim()),
  );
  const width = Math.max(...cells.map((r) => r.length));
  const heads = Array.from(rows[0].children);
  const align = Array.from({ length: width }, (_, i) => {
    const c = heads[i] as HTMLElement | undefined;
    const a = c?.getAttribute("align") ?? c?.style?.textAlign ?? "";
    return a === "left" ? ":--" : a === "right" ? "--:" : a === "center" ? ":-:" : "---";
  });
  const row = (r: string[]) => `| ${Array.from({ length: width }, (_, i) => r[i] ?? "").join(" | ")} |`;
  return [row(cells[0]), row(align), ...cells.slice(1).map(row)].join("\n");
}
