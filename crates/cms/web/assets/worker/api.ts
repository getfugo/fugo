// The API: one class per request, with the signed-in user, their roles and the git host; and draft
// ids. A draft is a branch under `cms/`: this editor's, or Decap CMS's (`cms/<collection>/<slug>`,
// which the editor shows, publishes and discards like its own).

import { areaOf, base64Size, cleanPath, commitMessage, mayEdit, parseTrailers } from "../common";
import { GitHub } from "./github";
import { type Author, type Body, type Change, type CompareFile, type Comparison, DRAFTS, GitError, HttpError, LABEL, MAX_CHANGES, MAX_PATCH, type Pull, Race, type Settings, type TreeEntry, type User } from "./shared";

export class Api {
  constructor(
    private readonly s: Settings,
    private readonly git: GitHub,
    private readonly user: User,
    private readonly now: () => number,
  ) {}

  me() {
    const u = this.user;
    return {
      email: u.email,
      roles: u.roles,
      edit: u.edit,
      publish: u.publish && this.s.workflow === "review",
      workflow: this.s.workflow,
      repo: this.s.git.repo,
      branch: this.s.git.branch,
      areas: this.s.areas,
      deny: this.s.deny,
      maxUpload: this.s.maxUpload,
    };
  }

  /** A file of the branch or of a draft, as base64 (`content`) with its blob id (`sha`). */
  async file(path: string | null, draft: string | null) {
    if (path === null || areaOf(this.s, path) === null) throw new HttpError(403, `the editor cannot open ${path}`);
    const ref = draft ? await this.draftHead(draft) : this.s.git.branch;
    const file = await this.git.read(ref, path);
    if (!file) throw new HttpError(404, `${path} does not exist`);
    return { path, ...file };
  }

  /** The open drafts, newest first, with their pull requests. */
  async drafts() {
    const [refs, pulls] = await Promise.all([this.git.branchesWithHeads(DRAFTS), this.pulls()]);
    return {
      drafts: refs
        .filter((r) => isDraftId(r.name))
        .map((r) => {
          const t = parseTrailers(r.message);
          const pr = pulls.get(DRAFTS + r.name) ?? null;
          return {
            id: r.name,
            entry: t["cms-entry"] ?? "",
            title: titleOf(t, pr, r.message),
            author: r.author,
            updated: r.date,
            pr,
          };
        }),
    };
  }

  /** A draft: its files (with their diff), its saves, and its files the branch changed since
   * (`conflicts`, which publishing merges). */
  async draft(id: unknown) {
    const head = await this.draftHead(id);
    const main = await this.mainHead();
    const branch = DRAFTS + String(id);
    const [cmp, pulls] = await Promise.all([this.git.compare(main, head), this.pulls(branch)]);
    const saves = savesOf(cmp);
    const last = saves.at(-1)?.message ?? "";
    const t = parseTrailers(last);
    const pr = pulls.get(branch) ?? null;
    return {
      id,
      entry: t["cms-entry"] ?? "",
      title: titleOf(t, pr, last),
      pr,
      files: cmp.files.map((f) => ({
        path: f.path,
        status: f.status,
        previous: f.previous,
        patch: f.patch && f.patch.length > MAX_PATCH ? `${f.patch.slice(0, MAX_PATCH)}\n…` : f.patch,
      })),
      commits: saves.map((c) => ({
        author: c.author,
        subject: c.message.split("\n")[0],
      })),
      conflicts: await this.conflicts(cmp, main),
    };
  }

  /** Commits changes to the draft of `entry` (or the branch, workflow `direct`): the draft the
   * page was opened from (`draft`), while it exists, else the page's own. */
  async save(body: Body) {
    const entry = typeof body.entry === "string" ? body.entry.trim() : "";
    if (!entry || entry.length > 512 || /[\u0000-\u001f]/.test(entry)) {
      throw new HttpError(400, "say which page the changes are for (entry)");
    }
    const changes: unknown[] = Array.isArray(body.changes) ? body.changes : [];
    if (changes.length === 0) throw new HttpError(400, "no changes");
    if (changes.length > MAX_CHANGES) throw new HttpError(413, `at most ${MAX_CHANGES} files at a time`);
    const seen = new Set<string>();
    const checked = changes.map((raw): Change => {
      const c = (raw ?? {}) as Body;
      const path = cleanPath(c.path);
      if (path === null || seen.has(path)) throw new HttpError(400, `bad path ${JSON.stringify(c.path)}`);
      seen.add(path);
      if (!mayEdit(this.s, this.user.edit, path)) throw new HttpError(403, `you may not change ${path}`);
      const base = c.base === undefined ? undefined : c.base === null ? null : String(c.base);
      if (c.delete === true) return { path, base, delete: true };
      if (c.from !== undefined) {
        const from = cleanPath(c.from);
        if (from === null || from === path) throw new HttpError(400, `bad move to ${path}`);
        if (!mayEdit(this.s, this.user.edit, from)) throw new HttpError(403, `you may not change ${from}`);
        return { path, base, from };
      }
      if (typeof c.content !== "string") throw new HttpError(400, `no content for ${path}`);
      const encoding = c.encoding === "base64" ? "base64" : "utf-8";
      const size = encoding === "base64" ? base64Size(c.content) : new TextEncoder().encode(c.content).length;
      if (size > this.s.maxUpload) throw new HttpError(413, `${path} is larger than the upload limit`);
      if (encoding === "base64" && !/^[A-Za-z0-9+/\s]*={0,2}\s*$/.test(c.content)) {
        throw new HttpError(400, `${path} is not base64`);
      }
      return { path, base, content: c.content, encoding };
    });
    // A move deletes its source in the same save: files are moved, never copied.
    const deleted = new Set(checked.filter((c) => c.delete).map((c) => c.path));
    for (const c of checked) {
      if (c.from !== undefined && !deleted.has(c.from)) throw new HttpError(400, `moving ${c.from} to ${c.path} must delete ${c.from}`);
    }
    const title = typeof body.title === "string" ? body.title : "";
    const verb = checked.every((c) => c.delete) ? "Delete" : checked.some((c) => c.from !== undefined) ? "Move" : "Edit";
    const message = commitMessage(`${verb} ${entry}${title ? `: ${title}` : ""}`, [
      ["CMS-Entry", entry],
      ["CMS-Title", title],
    ]);
    const author = { name: this.user.name, email: this.user.email };
    if (this.s.workflow === "direct") {
      const commit = await this.git.commitFiles(this.s.git.branch, null, checked, message, author, this.now);
      return { commit, draft: null };
    }
    const id = (await this.openDraft(body.draft)) ?? (await draftId(entry));
    const { commit, created } = await this.git.commitFiles(DRAFTS + id, this.s.git.branch, checked, message, author, this.now);
    if (created) await this.openPull(id, message);
    return { commit, draft: id };
  }

  /** Copies a draft's files onto the branch in one commit, then deletes the draft. When the branch
   * changed some of them since the draft was made, it is merged into the draft first (git's
   * merge: moved files and changes to different lines of a file merge). */
  async publish(body: Body) {
    if (this.s.workflow !== "review") throw new HttpError(400, "there are no drafts: workflow is direct");
    if (!this.user.publish) throw new HttpError(403, "you may not publish");
    const id = body.id;
    let merged = false;
    let attempt = 0;
    for (;;) {
      const head = await this.draftHead(id);
      const main = await this.mainHead();
      const cmp = await this.git.compare(main, head);
      if (cmp.files.length >= 300) throw new HttpError(422, "the draft changes too many files to publish here");
      for (const f of cmp.files) {
        for (const p of pathsOf(f)) {
          if (!mayEdit(this.s, this.user.edit, p)) {
            throw new HttpError(403, `the draft changes ${p}, which you may not change`);
          }
        }
      }
      const saves = savesOf(cmp);
      const last = saves.at(-1)?.message ?? "";
      const t = parseTrailers(last);
      const conflicts = await this.conflicts(cmp, main);
      if (conflicts.length > 0) {
        if (merged) throw new HttpError(409, "the branch keeps changing these files; try again", { conflicts });
        // The merge commit carries the draft's trailers: the drafts list reads its last commit's.
        const message = commitMessage(`Bring in ${this.s.git.branch}`, [
          ["CMS-Entry", t["cms-entry"]],
          ["CMS-Title", titleOf(t, null, last)],
        ]);
        if (!(await this.git.mergeInto(DRAFTS + id, this.s.git.branch, message))) {
          throw new HttpError(409, "the branch and the draft changed the same lines of these files", { conflicts });
        }
        merged = true;
        continue;
      }
      if (cmp.files.length === 0) {
        await this.git.deleteBranch(DRAFTS + id);
        return { commit: null, published: false };
      }
      const entries: TreeEntry[] = [];
      for (const f of cmp.files) {
        if (f.previous && f.previous !== f.path) entries.push({ path: f.previous, sha: null });
        entries.push({ path: f.path, sha: f.status === "removed" ? null : f.sha });
      }
      const authors: Author[] = [];
      for (const c of saves) {
        const a = c.author;
        if (a?.email && !authors.some((x) => x.email === a.email)) authors.push(a);
      }
      const author = authors[0] ?? { name: this.user.name, email: this.user.email };
      const entry = t["cms-entry"] ?? String(id);
      const message = commitMessage(`Publish ${entry}${t["cms-title"] ? `: ${t["cms-title"]}` : ""}`, [
        ["CMS-Entry", entry],
        ["CMS-Title", t["cms-title"]],
        ...authors.slice(1).map((a): [string, string] => ["Co-authored-by", `${a.name} <${a.email}>`]),
        ["CMS-Published-By", this.user.email],
      ]);
      try {
        const commit = await this.git.commitTree(this.s.git.branch, main, entries, message, author, this.now);
        // A save that reached the draft after it was compared stays a draft.
        if ((await this.git.head(DRAFTS + id)) !== head) return { commit, published: true, kept: true };
        // Deleting the branch closes its pull request, which then says where the draft went.
        const pr = (await this.pulls(DRAFTS + id)).get(DRAFTS + id);
        if (pr) await this.hostMay(() => this.git.comment(pr.number, `Published with the site's editor in ${commit}.`));
        await this.git.deleteBranch(DRAFTS + id);
        return { commit, published: true };
      } catch (e) {
        if (e instanceof Race && attempt++ < 2) continue;
        if (e instanceof Race) throw new HttpError(409, "the branch keeps changing; try again");
        throw e;
      }
    }
  }

  /** Deletes a draft: a publisher's, or one whose saves are all the user's. */
  async discard(body: Body) {
    const id = body.id;
    const head = await this.draftHead(id);
    if (!this.user.publish) {
      const cmp = await this.git.compare(await this.mainHead(), head);
      if (savesOf(cmp).some((c) => (c.author?.email ?? "").toLowerCase() !== this.user.email)) {
        throw new HttpError(403, "others edited this draft: ask someone who may publish to discard it");
      }
    }
    await this.git.deleteBranch(DRAFTS + id);
    return { discarded: id };
  }

  /** The open pull requests of drafts, by branch (`branch`'s, or all), or none when the git host
   * refuses them (a token without the permission). */
  private async pulls(branch?: string): Promise<Map<string, Pull>> {
    return (await this.hostMay(() => this.git.openPulls(branch))) ?? new Map();
  }

  /** Opens the pull request of a new draft, labelled `fugo-cms`, so that the draft shows on the
   * git host too. Without the permission there is none, and the draft stays as it is. */
  private async openPull(id: string, message: string): Promise<void> {
    const link = new URL(`${this.s.path}#/d/${id}`, this.s.site).href;
    const body = `A draft of the site's editor. Review it, and publish or discard it there: ${link}`;
    await this.hostMay(() => this.git.openPull(DRAFTS + id, this.s.git.branch, message.split("\n")[0], body, [LABEL]));
  }

  /** A call to the git host that may fail (pull requests: an extra the token may not allow);
   * undefined when it does. */
  private async hostMay<T>(call: () => Promise<T>): Promise<T | undefined> {
    try {
      return await call();
    } catch (e) {
      if (e instanceof GitError) return undefined;
      throw e;
    }
  }

  /** `id` when it names a draft that exists (a page opened from a draft that was published or
   * discarded since saves to a draft of its own). */
  private async openDraft(id: unknown): Promise<string | null> {
    if (id === undefined || id === null) return null;
    if (!isDraftId(id)) throw new HttpError(400, "bad draft id");
    return (await this.git.head(DRAFTS + id)) ? id : null;
  }

  private async draftHead(id: unknown): Promise<string> {
    if (!isDraftId(id)) throw new HttpError(400, "bad draft id");
    const head = await this.git.head(DRAFTS + id);
    if (!head) throw new HttpError(404, "no such draft (published or discarded?)");
    return head;
  }

  private async mainHead(): Promise<string> {
    const head = await this.git.head(this.s.git.branch);
    if (!head) throw new HttpError(500, `branch ${this.s.git.branch} does not exist`);
    return head;
  }

  /** The draft's paths the branch changed since the draft was made from it (or last merged it):
   * their blob at the merge base and on the branch differ (exact, however many files the branch
   * changed). */
  private async conflicts(cmp: Comparison, main: string): Promise<string[]> {
    if (cmp.mergeBase === main) return [];
    const paths = [...new Set(cmp.files.flatMap(pathsOf))];
    const [then, nowIds] = await Promise.all([this.git.blobIds(cmp.mergeBase, paths), this.git.blobIds(main, paths)]);
    return paths.filter((p) => then[p] !== nowIds[p]);
  }
}

/** A changed file's path, and its previous path when it moved. */
const pathsOf = (f: CompareFile): string[] => (f.previous ? [f.path, f.previous] : [f.path]);

/** A draft's saves: its commits but the merges that brought the branch in (the bot's). */
const savesOf = (cmp: Comparison) => cmp.commits.filter((c) => !c.merge);

/** A draft id: this editor's (`draftId`), or Decap CMS's (`<collection>/<slug>`). Its segments
 * start with a letter, digit, `_` or `-` (no `.` or `..`: ids go into the git host's URLs). */
const isDraftId = (id: unknown): id is string =>
  typeof id === "string" && id.length <= 200 && /^[\p{L}\p{M}\p{N}_-][\p{L}\p{M}\p{N}_.-]*(?:\/[\p{L}\p{M}\p{N}_-][\p{L}\p{M}\p{N}_.-]*)*$/u.test(id);

/** A draft's title: the page's, from its last commit; for a draft of Decap CMS, its pull
 * request's, or the commit's subject (a merge's `CMS-Title`, which keeps its last save's). */
function titleOf(trailers: Record<string, string>, pr: Pull | null, message: string): string {
  if (trailers["cms-entry"] !== undefined) return trailers["cms-title"] ?? "";
  return pr?.title ?? trailers["cms-title"] ?? message.split("\n")[0];
}

/** The draft id of a page: its key as a slug, and 8 hex digits of its SHA-256 (keys that slug
 * alike stay apart). */
export async function draftId(entry: string): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(entry)));
  const hash = Array.from(digest.slice(0, 4), (b) => b.toString(16).padStart(2, "0")).join("");
  const slug = entry
    .replace(/(^|\/)_index$/, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60)
    .replace(/-+$/, "");
  return `${slug || "home"}-${hash}`;
}
