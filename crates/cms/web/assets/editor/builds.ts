// The build of the open page's draft: whether the site's build of the draft's last save is done,
// with a link to the page in it (the API's `GET build`, from the deployments the git host lists,
// which a host or a CI job that builds the drafts' branches makes). It shows beside the draft's
// badge, and is asked about again while it is under way.

import { html, nothing, render, type TemplateResult } from "lit-html";
import { ref } from "lit-html/directives/ref.js";
import { api } from "./api";
import { type Build, type Draft } from "./data";
import { icon } from "./icons";
import { current } from "./state";

/** How often a build under way is asked about again, and how many times at most (half an hour). */
const POLL_MS = 20_000;
const POLLS = 90;

/** The draft whose build is shown, what the API last said of it, and where it shows. */
interface Watch {
  draft: Draft;
  build: Build | null;
  asked: number;
  /** A question is under way. */
  busy: boolean;
  el: HTMLElement | null;
  timer?: ReturnType<typeof setTimeout>;
}

let watch: Watch | null = null;

/** Where the status of `draft`'s build shows (nothing without a draft). A draft loaded anew (after
 * a save) is asked about anew. */
export function buildStatus(draft: Draft | null): TemplateResult | typeof nothing {
  if (watch && watch.draft !== draft) {
    clearTimeout(watch.timer);
    watch = null;
  }
  if (!draft) return nothing;
  const w = (watch ??= { draft, build: null, asked: 0, busy: false, el: null });
  const place = (el?: Element) => {
    w.el = (el as HTMLElement | undefined) ?? null;
    draw(w);
    // Asked about when first shown, and again when shown after the page was left while the
    // build was under way.
    const again = underWay(w.build) && w.asked < POLLS;
    if (w.el && !w.busy && w.timer === undefined && (w.asked === 0 || again)) void ask(w);
  };
  return html`<span class="build" ${ref(place)}></span>`;
}

const underWay = (b: Build | null) => b?.state === "building" || b?.state === "waiting";

async function ask(w: Watch): Promise<void> {
  w.asked++;
  w.busy = true;
  try {
    w.build = await api<Build>("GET", "build", { draft: w.draft.id });
  } catch {
    // An API without builds (an older Worker), or a draft gone since: nothing to show.
    w.build = null;
  } finally {
    w.busy = false;
  }
  if (watch !== w) return;
  draw(w);
  if (underWay(w.build) && w.asked < POLLS && w.el?.isConnected) {
    w.timer = setTimeout(() => {
      w.timer = undefined;
      if (watch === w && w.el?.isConnected) void ask(w);
    }, POLL_MS);
  }
}

function draw(w: Watch): void {
  if (w.el) render(view(w.build), w.el);
}

/** `u` when it is an http or https URL. */
const web = (u: string | null) => (u && /^https?:\/\//i.test(u) ? u : null);

/** The open page in the build served at `root`: at its address below the site's root, or the
 * build's root for a page without one yet. */
function pageIn(root: string): string {
  const p = current();
  const path = p.entry.files.find((f) => f.lang === p.lang)?.url;
  const base = root.endsWith("/") ? root : `${root}/`;
  return path ? new URL(path.replace(/^\//, ""), base).href : base;
}

function view(b: Build | null): TemplateResult | typeof nothing {
  const link = (url: string, kind: string, text: string, title: string) =>
    html`<a class="badge ${kind}" href=${url} target="_blank" rel="noopener" title=${title}>${text}${icon("open_in_new")}</a>`;
  const url = web(b?.url ?? null);
  const log = web(b?.log ?? null);
  switch (b?.state) {
    case "building":
      return html`<span class="badge" title="The site is being built with the draft's last save">Building the draft…</span>`;
    case "ready":
      return url
        ? link(pageIn(url), "added", "Draft built", "This page as the site's build of the draft's last save shows it")
        : html`<span class="badge added">Draft built</span>`;
    case "failed":
      return log ? link(log, "removed", "The draft's build failed", "The build's log") : html`<span class="badge removed">The draft's build failed</span>`;
    case "waiting":
      return html`<span class="badge" title="The site's build of the draft's last save has not started">Not built yet</span>${url
          ? link(pageIn(url), "", "Earlier build", "This page as the build of an earlier save shows it")
          : nothing}`;
    default:
      return nothing;
  }
}
