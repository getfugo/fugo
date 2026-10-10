// Rendering: the editor's frame (header and sidebar) around a view, and toasts.

import { html, render } from "lit-html";
import { reveal, sidebar } from "./tree";
import { header } from "./views";

/** The value of the input an event came from. */
export const valueOf = (ev: Event) => (ev.target as HTMLInputElement).value;

export function app(): HTMLElement {
  const el = document.getElementById("app");
  if (!el) throw new Error("no #app");
  return el;
}

let mounted = false;

/** Shows `content` as the editor's main part, in its frame. lit-html changes only what differs
 * from what is shown, so inputs keep their focus and text across a redraw. */
export function show(content: unknown): void {
  const root = app();
  if (!mounted) {
    // The page's own placeholder goes; from now on the editor owns the element.
    root.replaceChildren();
    root.className = "";
    mounted = true;
  }
  render(html`${header()}<div class="layout"><nav class="sidebar" aria-label="Folders"></nav><main>${content}</main></div>`, root);
  drawSidebar();
}

/** Draws the sidebar, on its own: a folder opened or closed in its tree changes nothing else. */
export function drawSidebar(): void {
  const nav = app().querySelector<HTMLElement>(".layout > .sidebar");
  if (!nav) return;
  render(sidebar(), nav);
  reveal(nav);
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(message: string, kind: "info" | "ok" | "error" = "info"): void {
  let el = document.getElementById("toast");
  if (!el) {
    el = document.createElement("div");
    el.id = "toast";
    el.setAttribute("role", "status");
    document.body.append(el);
  }
  const box = el;
  box.className = `toast ${kind}`;
  box.textContent = message;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (box.className = "toast hidden"), kind === "error" ? 9000 : 4000);
}
