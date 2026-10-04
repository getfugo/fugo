// DOM helpers: elements, rendering and toasts.

import { header, sidebar } from "./views";

type Child = Node | string | number | null | undefined | false | Child[];

/** An element: `h("a", {href, onclick}, "text", child)`. Strings are text, never HTML. */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, unknown> | null = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs ?? {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v as EventListener);
    else if (k === "class") el.className = String(v);
    else if (k === "value") (el as unknown as HTMLInputElement).value = String(v);
    else if (k === "checked") (el as unknown as HTMLInputElement).checked = Boolean(v);
    else el.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of (children as unknown[]).flat(Infinity)) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

/** The value of the input an event came from. */
export const valueOf = (ev: Event) => (ev.target as HTMLInputElement).value;

export function app(): HTMLElement {
  const el = document.getElementById("app");
  if (!el) throw new Error("no #app");
  return el;
}

export function render(...children: Child[]): void {
  const root = app();
  root.className = "";
  root.replaceChildren(header(), h("div", { class: "layout" }, sidebar(), h("main", {}, ...children)));
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(message: string, kind: "info" | "ok" | "error" = "info"): void {
  let el = document.getElementById("toast");
  if (!el) {
    el = h("div", { id: "toast", role: "status" });
    document.body.append(el);
  }
  const box = el;
  box.className = `toast ${kind}`;
  box.textContent = message;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (box.className = "toast hidden"), kind === "error" ? 9000 : 4000);
}
