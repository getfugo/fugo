// Questions in a dialog, in place of the browser's prompt() and confirm(): a text to type (a new
// page's title, a link's address), or whether to go on with a change that is hard to undo.

import { html, nothing, render } from "lit-html";
import { type MdDialog, type MdOutlinedTextField } from "./material";

interface Question {
  title: string;
  /** What the question is about, under its title. */
  text?: string;
  /** The label of the field to type in; none for a yes or no. */
  field?: string;
  value?: string;
  /** The label of the button that answers yes. */
  yes: string;
  /** The change is hard to undo: the button that makes it has the error colour. */
  danger?: boolean;
}

/** Shows `q` in a dialog, and resolves to what was typed (`""` without a field) when it is answered
 * yes; null when it is cancelled (also by Escape, or a click beside it). */
function ask(q: Question): Promise<string | null> {
  const host = document.createElement("div");
  document.body.append(host);
  return new Promise((resolve) => {
    let answer: string | null = null;
    const dialog = () => host.querySelector<MdDialog>("md-dialog");
    const close = (yes: boolean) => {
      answer = yes ? (host.querySelector<MdOutlinedTextField>("md-outlined-text-field")?.value ?? "") : null;
      void dialog()?.close(yes ? "yes" : "no");
    };
    const enter = (ev: KeyboardEvent) => {
      if (ev.key !== "Enter") return;
      ev.preventDefault();
      close(true);
    };
    const closed = () => {
      render(nothing, host);
      host.remove();
      resolve(answer);
    };
    render(
      html`
        <md-dialog class=${q.danger ? "danger" : nothing} @closed=${closed}>
          <div slot="headline">${q.title}</div>
          <div slot="content">
            ${q.text ? html`<p>${q.text}</p>` : nothing}
            ${q.field !== undefined ? html`<md-outlined-text-field label=${q.field} .value=${q.value ?? ""} autofocus @keydown=${enter}></md-outlined-text-field>` : nothing}
          </div>
          <div slot="actions">
            <md-text-button class="no" @click=${() => close(false)}>Cancel</md-text-button>
            <md-filled-button class="yes" @click=${() => close(true)}>${q.yes}</md-filled-button>
          </div>
        </md-dialog>
      `,
      host,
    );
    void dialog()?.show();
  });
}

/** Asks for a text: what was typed in the field `field` (starting as `value`), or null. */
export const askText = (title: string, field: string, yes: string, value = "") => ask({ title, field, value, yes });

/** Asks whether to go on: true when the button `yes` was clicked. */
export const askYes = async (title: string, text: string, yes: string, danger = false) => (await ask({ title, text, yes, danger })) !== null;
