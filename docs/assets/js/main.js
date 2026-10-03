// The fugo documentation's script, bundled by `js_build` (in process, no Node.js tools):
// client-side navigation between pages, the colour scheme toggle, the navigation drawer, copy
// buttons, configuration tabs, the table of contents' active heading, and the search dialog.
//
// Clicks are handled by delegation on the document, so the controls keep working after a
// navigation replaces the page; what depends on the page's content runs again in `initPage`.

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// ── Colour scheme ────────────────────────────────────────────────────────────────────────
function toggleTheme() {
  const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
  document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem("fugo-theme", next);
  } catch (e) {
    // Private mode: the choice lasts for this page only.
  }
}

// ── Navigation drawer (narrow screens) ───────────────────────────────────────────────────
function setNav(open) {
  const nav = $("[data-nav]");
  if (nav) nav.classList.toggle("is-open", open);
  const button = $("[data-nav-toggle]");
  if (button) button.setAttribute("aria-expanded", String(open));
}

// Scrolls the sidebar (only the sidebar, not the page) so that the current page's link shows.
function revealCurrent(center) {
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

function isOpening(details) {
  return sectionTargets.has(details) ? sectionTargets.get(details) : details.open;
}

function setOpen(details, open) {
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
function syncSidebar(keep, incoming) {
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
async function copyCode(button) {
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
function selectFormat(format) {
  for (const tabs of $$("[data-tabs]")) {
    for (const b of $$("[data-tab]", tabs)) {
      b.setAttribute("aria-selected", String(b.dataset.tab === format));
    }
    for (const p of $$("[data-panel]", tabs)) {
      p.hidden = p.dataset.panel !== format;
    }
  }
}

function restoreFormat() {
  try {
    const saved = localStorage.getItem("fugo-config-format");
    if (saved) selectFormat(saved);
  } catch (e) {
    // ignore
  }
}

// ── Table of contents: the heading in view ───────────────────────────────────────────────
let tocObserver = null;

function initToc() {
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

// ── Client-side navigation ───────────────────────────────────────────────────────────────
// A click on a link to another page of the site fetches that page and replaces the part of the
// document that differs (`[data-page]`, the header when it changed, the title), instead of
// loading the whole document again. Pages are prefetched when the pointer rests on a link.
// Anything unexpected (another origin, a file, an error, a page without `[data-page]`) falls
// back to an ordinary navigation.

const pageCache = new Map(); // URL without fragment → { time, html: Promise<string> }
const CACHE_MS = 60_000;
let currentPath = location.pathname + location.search;
let navigation = 0;

function pageURL(href) {
  const url = new URL(href, location.href);
  if (url.origin !== location.origin) return null;
  // The documentation's pages end in a slash; other paths are files (JSON, XML, images).
  if (!url.pathname.endsWith("/") && !url.pathname.endsWith(".html")) return null;
  return url;
}

function linkTarget(a) {
  if (!a || !a.href || a.hasAttribute("download") || a.dataset.reload !== undefined) return null;
  if (a.target && a.target !== "_self") return null;
  return pageURL(a.href);
}

function fetchPage(url) {
  const key = url.origin + url.pathname + url.search;
  const hit = pageCache.get(key);
  if (hit && Date.now() - hit.time < CACHE_MS) return hit.html;
  const html = fetch(key, { headers: { Accept: "text/html" } }).then((res) => {
    const type = res.headers.get("content-type") || "";
    if (!res.ok || !type.includes("text/html")) throw new Error(`${res.status} ${type}`);
    return res.text();
  });
  html.catch(() => pageCache.delete(key));
  pageCache.set(key, { time: Date.now(), html });
  if (pageCache.size > 40) pageCache.delete(pageCache.keys().next().value);
  return html;
}

function syncHead(doc) {
  document.title = doc.title;
  for (const sel of ['meta[name="description"]', 'link[rel="canonical"]']) {
    const now = $(sel);
    const next = $(sel, doc);
    if (now && next) now.replaceWith(next.cloneNode(true));
  }
}

function scrollAfterSwap(url, scroll) {
  if (scroll != null) {
    window.scrollTo(0, scroll);
    return;
  }
  const target = url.hash && document.getElementById(decodeURIComponent(url.hash.slice(1)));
  if (target) target.scrollIntoView();
  else window.scrollTo(0, 0);
}

async function navigate(url, { push = true, scroll = null } = {}) {
  const id = ++navigation;
  const search = $("[data-search]");
  if (search?.open) search.close();
  let html;
  try {
    html = await fetchPage(url);
  } catch (e) {
    location.assign(url.href);
    return;
  }
  if (id !== navigation) return; // a later click won
  const doc = new DOMParser().parseFromString(html, "text/html");
  const next = $("[data-page]", doc);
  const now = $("[data-page]");
  if (!next || !now) {
    location.assign(url.href);
    return;
  }
  if (push) {
    saveScroll();
    history.pushState({ scroll: 0 }, "", url.href);
  }
  currentPath = url.pathname + url.search;

  const swap = () => {
    syncHead(doc);
    document.body.className = doc.body.className;
    const header = $(".site-header");
    const nextHeader = $(".site-header", doc);
    if (header && nextHeader && header.innerHTML !== nextHeader.innerHTML) {
      header.replaceWith(document.adoptNode(nextHeader));
    }
    const page = document.adoptNode(next);
    const sidebar = $("[data-nav]");
    const incoming = $("[data-nav]", page);
    let toOpen = [];
    if (sidebar && incoming) {
      const top = sidebar.scrollTop;
      toOpen = syncSidebar(sidebar, incoming);
      incoming.replaceWith(sidebar);
      $("[data-page]").replaceWith(page);
      sidebar.scrollTop = top;
    } else {
      $("[data-page]").replaceWith(page);
    }
    scrollAfterSwap(url, scroll);
    initPage(false);
    Promise.all(toOpen.map((d) => setOpen(d, true))).then(() => revealCurrent(false));
    // For screen readers and keyboard users: the new page's content, as after a page load.
    const main = document.getElementById("main");
    if (main) {
      main.setAttribute("tabindex", "-1");
      main.focus({ preventScroll: true });
    }
  };
  if (document.startViewTransition && !reducedMotion()) document.startViewTransition(swap);
  else swap();
}

// The scroll position of the current history entry, for the back and forward buttons.
let scrollTimer = 0;
function saveScroll() {
  clearTimeout(scrollTimer);
  try {
    history.replaceState({ ...(history.state || {}), scroll: window.scrollY }, "");
  } catch (e) {
    // Too many history updates (Safari's rate limit): the position is not kept.
  }
}

function initNavigation() {
  if (!("pushState" in history) || !window.DOMParser) return;
  history.scrollRestoration = "manual";
  const saved = history.state?.scroll;
  if (saved != null && !location.hash) window.scrollTo(0, saved);

  document.addEventListener("click", (e) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const url = linkTarget(e.target.closest("a"));
    if (!url) return;
    // A link to a heading of this page: the browser scrolls.
    if (url.pathname + url.search === currentPath && url.hash) return;
    e.preventDefault();
    if (url.href === location.href) {
      window.scrollTo(0, 0);
      return;
    }
    navigate(url);
  });

  let hoverTimer = 0;
  const prefetch = (e) => {
    const a = e.target.closest?.("a");
    const url = linkTarget(a);
    if (!url || url.pathname + url.search === currentPath) return;
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => fetchPage(url).catch(() => {}), e.type === "touchstart" ? 0 : 65);
  };
  document.addEventListener("mouseover", prefetch);
  document.addEventListener("touchstart", prefetch, { passive: true });
  document.addEventListener("mouseout", () => clearTimeout(hoverTimer));

  window.addEventListener("popstate", (e) => {
    const url = new URL(location.href);
    // Back to another heading of the same page: the browser scrolls.
    if (url.pathname + url.search === currentPath) return;
    navigate(url, { push: false, scroll: e.state?.scroll ?? 0 });
  });

  window.addEventListener("scroll", () => {
    clearTimeout(scrollTimer);
    scrollTimer = setTimeout(saveScroll, 200);
  }, { passive: true });
  window.addEventListener("pagehide", saveScroll);
}

// ── Search ───────────────────────────────────────────────────────────────────────────────
const escapeHtml = (s) =>
  s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

function highlight(text, terms) {
  let html = escapeHtml(text);
  for (const t of terms) {
    if (!t) continue;
    const re = new RegExp(`(${t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "ig");
    html = html.replace(re, "<mark>$1</mark>");
  }
  return html;
}

function excerpt(text, terms) {
  const lower = text.toLowerCase();
  let at = -1;
  for (const t of terms) {
    at = lower.indexOf(t);
    if (at >= 0) break;
  }
  if (at < 0) return text.slice(0, 140);
  const start = Math.max(0, at - 50);
  return (start > 0 ? "…" : "") + text.slice(start, start + 160) + "…";
}

function score(entry, terms) {
  const title = entry.t.toLowerCase();
  const desc = (entry.d || "").toLowerCase();
  const body = (entry.x || "").toLowerCase();
  let total = 0;
  for (const t of terms) {
    let s = 0;
    if (title === t) s += 100;
    else if (title.startsWith(t)) s += 60;
    else if (title.includes(t)) s += 40;
    if (desc.includes(t)) s += 12;
    if (body.includes(t)) s += 4 + Math.min(6, body.split(t).length - 1);
    if (s === 0) return 0;
    total += s;
  }
  if (entry.k === "section") total += 5;
  return total;
}

function initSearch() {
  const dialog = $("[data-search]");
  const input = $("[data-search-input]");
  const results = $("[data-search-results]");
  const hint = $("[data-search-hint]");
  if (!dialog || !input || !results) return null;
  let index = null;
  let selected = -1;

  const load = async () => {
    if (index) return index;
    const res = await fetch(results.dataset.index);
    index = await res.json();
    return index;
  };

  const render = async () => {
    const q = input.value.trim().toLowerCase();
    const terms = q.split(/\s+/).filter(Boolean);
    selected = -1;
    if (!terms.length) {
      results.innerHTML = "";
      hint.hidden = false;
      hint.textContent = "Type to search titles, headings and text.";
      return;
    }
    const entries = await load();
    const hits = entries
      .map((e) => [score(e, terms), e])
      .filter(([s]) => s > 0)
      .sort((a, b) => b[0] - a[0])
      .slice(0, 12);
    hint.hidden = hits.length > 0;
    hint.textContent = hits.length ? "" : `No results for “${input.value.trim()}”.`;
    results.innerHTML = hits
      .map(
        ([, e]) => `<li><a href="${e.u}">
          ${e.s ? `<span class="search__section">${escapeHtml(e.s.replace(/-/g, " "))}</span>` : ""}
          <span class="search__title">${highlight(e.t, terms)}</span>
          <span class="search__excerpt">${highlight(excerpt(e.d || e.x || "", terms), terms)}</span>
        </a></li>`,
      )
      .join("");
  };

  const move = (delta) => {
    const links = $$("a", results);
    if (!links.length) return;
    selected = (selected + delta + links.length) % links.length;
    links.forEach((a, i) => a.setAttribute("aria-selected", String(i === selected)));
    links[selected].scrollIntoView({ block: "nearest" });
  };

  const open = () => {
    if (!dialog.open) dialog.showModal();
    input.select();
    load();
  };

  document.addEventListener("keydown", (e) => {
    const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName || "");
    if ((e.key === "/" && !typing) || (e.key === "k" && (e.metaKey || e.ctrlKey))) {
      e.preventDefault();
      open();
    }
  });
  input.addEventListener("input", render);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "Enter") {
      const links = $$("a", results);
      const target = links[selected] || links[0];
      if (target) {
        e.preventDefault();
        const url = linkTarget(target);
        if (url) navigate(url);
        else location.assign(target.href);
      }
    }
  });
  dialog.addEventListener("click", (e) => {
    if (e.target === dialog) dialog.close();
  });
  return open;
}

// ── Wiring ───────────────────────────────────────────────────────────────────────────────
// What depends on the page's content; `first` is true on a full load.
function initPage(first) {
  setNav(false);
  revealCurrent(first);
  restoreFormat();
  initToc();
}

document.addEventListener("DOMContentLoaded", () => {
  const openSearch = initSearch();
  document.addEventListener("click", (e) => {
    // The chevron (or anywhere in a sidebar row but its link) opens or closes the section.
    const summary = e.target.closest(".sidebar summary");
    if (summary && !e.target.closest("a")) {
      e.preventDefault();
      const details = summary.parentElement;
      setOpen(details, !isOpening(details));
      return;
    }
    const el = e.target.closest("[data-theme-toggle], [data-nav-toggle], [data-copy], [data-tab], [data-search-open]");
    if (!el) return;
    if (el.matches("[data-theme-toggle]")) toggleTheme();
    else if (el.matches("[data-nav-toggle]")) setNav(!$("[data-nav]")?.classList.contains("is-open"));
    else if (el.matches("[data-copy]")) copyCode(el);
    else if (el.matches("[data-search-open]")) openSearch?.();
    else {
      selectFormat(el.dataset.tab);
      try {
        localStorage.setItem("fugo-config-format", el.dataset.tab);
      } catch (err) {
        // ignore
      }
    }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") setNav(false);
  });
  initNavigation();
  initPage(true);
});
