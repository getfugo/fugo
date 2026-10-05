// The drafts: their list, and a draft's changes as a diff.

import { html, nothing, type TemplateResult } from "lit-html";
import { api, messageOf } from "./api";
import { discard, publish } from "./changes";
import { type Draft, type DraftDetail } from "./data";
import { show, toast } from "./dom";
import { drafts, me, pending, site } from "./state";

export function draftList(list: Draft[]): TemplateResult {
  if (list.length === 0) return html`<p class="muted">No drafts.</p>`;
  return html`
    <table class="entries">
      <tbody>
        ${list.map(
          (d) => html`
            <tr>
              <td><a href="#/d/${d.id}">${d.title || d.entry}</a></td>
              <td class="muted">${d.author?.email ?? ""}</td>
              <td class="muted">${d.updated ? new Date(d.updated).toLocaleString() : ""}</td>
            </tr>
          `,
        )}
      </tbody>
    </table>
  `;
}

export function draftsView(): void {
  show(html`
    <h1>Drafts</h1>
    ${draftList(drafts)}
  `);
}

export async function draftView(id: string): Promise<void> {
  show(html`<p class="muted">Loading…</p>`);
  let d: DraftDetail;
  try {
    d = await api<DraftDetail>("GET", "draft", { id });
  } catch (e) {
    toast(messageOf(e), "error");
    return draftsView();
  }
  const known = site.entries.some((e) => e.key === d.entry) || pending.has(d.entry);
  show(html`
    <div class="crumbs"><a href="#/drafts">Drafts</a> / ${d.entry}</div>
    <h1>${d.title || d.entry}</h1>
    ${d.conflicts.length
      ? html`<p class="warn">The site changed ${d.conflicts.join(", ")} since this draft was made: open the page, redo the changes, and discard this draft.</p>`
      : nothing}
    <h2>Changes</h2>
    ${d.files.map(
      (f) => html`
        <details class="change" ?open=${d.files.length <= 3}>
          <summary><span class="badge ${f.status}">${f.status}</span> <code>${f.previous ? `${f.previous} → ${f.path}` : f.path}</code></summary>
          ${f.patch ? diff(f.patch) : html`<p class="muted">A binary file, or too large to show.</p>`}
        </details>
      `,
    )}
    <h2>Saves</h2>
    <ul>
      ${d.commits.map((c) => html`<li><span class="muted">${c.author?.email ?? ""}</span> — ${c.subject}</li>`)}
    </ul>
    <div class="actions">
      ${known ? html`<a class="button" href="#/e/${encodeURIComponent(d.entry)}">Open the page</a>` : nothing}
      ${me.publish ? html`<button class="primary" ?disabled=${d.conflicts.length > 0} @click=${() => publish(id)}>Publish</button>` : nothing}
      <button class="danger" @click=${() => discard(id)}>Discard</button>
    </div>
  `);
}

const lineClass = (line: string) => (line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : line.startsWith("@@") ? "hunk" : nothing);

/** A patch, its added, deleted and hunk lines marked. In `<pre>`: no whitespace of its own. */
function diff(patch: string): TemplateResult {
  return html`<pre class="diff">${patch.split("\n").map((line) => html`<span class=${lineClass(line)}>${`${line}\n`}</span>`)}</pre>`;
}
