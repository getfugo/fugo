// The controls of a page: the colour scheme, the navigation drawer, the sidebar's sections, copy
// buttons, configuration tabs and the table of contents' active heading.

import { $, $$, reducedMotion } from "./dom.js";

// ── Colour scheme ────────────────────────────────────────────────────────────────────────
export function toggleTheme() {
  const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
  document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem("fugo-theme", next);
  } catch (e) {
    // Private mode: the choice lasts for this page only.
  }
}

// ── Navigation drawer (narrow screens) ───────────────────────────────────────────────────
export function setNav(open) {
  const nav = $("[data-nav]");
  if (nav) nav.classList.toggle("is-open", open);
  const button = $("[data-nav-toggle]");
  if (button) button.setAttribute("aria-expanded", String(open));
}

// Scrolls the sidebar (only the sidebar, not the page) so that the current page's link shows.
export function revealCurrent(center) {
  const nav = $("[data-nav]");
  const current = nav && $('[aria-current="page"]', nav);
  if (!current) return;
  const top = current.getBoundingClientRect().top - nav.getBoundingClientRect().top + nav.scrollTop;
  const visible = top >= nav.scrollTop && top + current.offsetHeight <= nav.scrollTop + nav.clientHeight;
  if (center || !visible) nav.scrollTop = top - nav.clientHeight / 2;
}

// ── Sidebar sections: animated opening and closing ───────────────────────────────────────
// A section is a `details` whose list grows or shrinks to its height; while an animation runs
// the element stays open, and a second click reverses it from where it is.
const sectionAnimations = new WeakMap(); // details → Animation
const sectionTargets = new WeakMap(); // details → the state it is animating to

export function isOpening(details) {
  return sectionTargets.has(details) ? sectionTargets.get(details) : details.open;
}

export function setOpen(details, open) {
  const list = $(":scope > ul", details);
  const running = sectionAnimations.get(details);
  if (isOpening(details) === open && (running || details.open === open)) return Promise.resolve();
  if (!list || !list.animate || reducedMotion()) {
    running?.cancel();
    sectionAnimations.delete(details);
    sectionTargets.delete(details);
    details.open = open;
    return Promise.resolve();
  }
  const from = running || details.open ? list.getBoundingClientRect().height : 0;
  running?.cancel();
  details.open = true;
  list.style.overflow = "hidden";
  const to = open ? list.scrollHeight : 0;
  const anim = list.animate(
    [
      { height: `${from}px`, opacity: from ? 1 : 0 },
      { height: `${to}px`, opacity: open ? 1 : 0 },
    ],
    { duration: Math.min(320, 160 + Math.abs(to - from) / 5), easing: "cubic-bezier(0.2, 0, 0, 1)" },
  );
  sectionAnimations.set(details, anim);
  sectionTargets.set(details, open);
  return anim.finished.then(
    () => {
      if (sectionAnimations.get(details) !== anim) return;
      sectionAnimations.delete(details);
      sectionTargets.delete(details);
      list.style.overflow = "";
      details.open = open;
    },
    () => {}, // cancelled by a later click
  );
}

// The sidebar is the same on every page but for the current link and the open sections: a
// navigation keeps the one on screen (its scroll position and the sections the reader opened)
// and takes only the new page's current link. Returns the sections to open for the new page.
export function syncSidebar(keep, incoming) {
  const href = $('[aria-current="page"]', incoming)?.getAttribute("href");
  for (const a of $$('[aria-current="page"]', keep)) a.removeAttribute("aria-current");
  const current = href && $$("a", keep).find((a) => a.getAttribute("href") === href);
  if (current) current.setAttribute("aria-current", "page");
  const byHref = new Map($$("details", keep).map((d) => [$(":scope > summary > a", d)?.getAttribute("href"), d]));
  return $$("details[open]", incoming)
    .map((d) => byHref.get($(":scope > summary > a", d)?.getAttribute("href")))
    .filter(Boolean);
}

// ── Copy buttons ─────────────────────────────────────────────────────────────────────────
export async function copyCode(button) {
  const code = $("pre", button.closest(".codeblock"));
  if (!code) return;
  try {
    await navigator.clipboard.writeText(code.innerText.replace(/\n$/, ""));
    button.classList.add("is-copied");
    setTimeout(() => button.classList.remove("is-copied"), 1500);
  } catch (e) {
    // No clipboard access (insecure context): nothing to do.
  }
}

// ── Configuration tabs ───────────────────────────────────────────────────────────────────
export function selectFormat(format) {
  for (const tabs of $$("[data-tabs]")) {
    for (const b of $$("[data-tab]", tabs)) {
      b.setAttribute("aria-selected", String(b.dataset.tab === format));
    }
    for (const p of $$("[data-panel]", tabs)) {
      p.hidden = p.dataset.panel !== format;
    }
  }
}

export function restoreFormat() {
  try {
    const saved = localStorage.getItem("fugo-config-format");
    if (saved) selectFormat(saved);
  } catch (e) {
    // ignore
  }
}

// ── Table of contents: the heading in view ───────────────────────────────────────────────
let tocObserver = null;

export function initToc() {
  if (tocObserver) tocObserver.disconnect();
  tocObserver = null;
  const links = $$(".toc a");
  if (!links.length || !("IntersectionObserver" in window)) return;
  const byId = new Map(links.map((a) => [decodeURIComponent(a.hash.slice(1)), a]));
  const headings = Array.from(byId.keys())
    .map((id) => document.getElementById(id))
    .filter(Boolean);
  const visible = new Set();
  const update = () => {
    const first = headings.find((h) => visible.has(h));
    for (const a of links) a.classList.remove("is-active");
    if (first) byId.get(first.id).classList.add("is-active");
  };
  tocObserver = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) visible.add(e.target);
        else visible.delete(e.target);
      }
      update();
    },
    { rootMargin: "-64px 0px -60% 0px" },
  );
  for (const h of headings) tocObserver.observe(h);
}

