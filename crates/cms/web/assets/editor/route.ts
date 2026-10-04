// Routing: the view the address names.

import { draftsView, draftView } from "./drafts";
import { openPage } from "./pages";
import { home, sectionView } from "./views";

export function route(): void {
  const hash = location.hash;
  let m: RegExpExecArray | null;
  if ((m = /^#\/s\/(.*)$/.exec(hash))) return sectionView(decodeURIComponent(m[1]));
  if ((m = /^#\/e\/(.+)$/.exec(hash))) return void openPage(decodeURIComponent(m[1]));
  if (hash === "#/drafts") return draftsView();
  if ((m = /^#\/d\/([a-z0-9-]+)$/.exec(hash))) return void draftView(m[1]);
  return home();
}
