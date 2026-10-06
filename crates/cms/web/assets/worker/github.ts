// Git hosts. A host has the methods of `GitHub`: head, branchesWithHeads, read, blobIds,
// commitFiles, commitTree, compare, deleteBranch. Paths are project-relative; the host adds
// `git.dir`.

import { type Author, type Change, type Comparison, type Fetch, GitError, HttpError, type Json, Race, type Settings, type TreeEntry, USER_AGENT } from "./shared";

const encodeSegments = (p: string) => p.split("/").map(encodeURIComponent).join("/");

/** GitHub's REST and GraphQL APIs. */
export class GitHub {
  private readonly repo: string;
  private readonly dir: string;
  private readonly owner: string;
  private readonly name: string;

  constructor(
    git: Settings["git"],
    private readonly token: () => Promise<string>,
    private readonly fetcher: Fetch,
  ) {
    this.repo = git.repo;
    this.dir = git.dir ?? "";
    [this.owner, this.name] = git.repo.split("/");
  }

  private async request(method: string, url: string, body?: unknown): Promise<Response> {
    return this.fetcher(url, {
      method,
      headers: {
        authorization: `Bearer ${await this.token()}`,
        accept: "application/vnd.github+json",
        "x-github-api-version": "2022-11-28",
        "user-agent": USER_AGENT,
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : typeof body === "string" ? body : JSON.stringify(body),
    });
  }

  /** A REST call on the repository. `missing`: the answer to a 404; `race`: a 422 means the
   * branch moved or exists (ref updates). */
  private async api(
    method: string,
    path: string,
    body?: unknown,
    { missing, race }: { missing?: null; race?: boolean } = {},
  ): Promise<Json> {
    const res = await this.request(method, `https://api.github.com/repos/${this.repo}${path}`, body);
    if (res.status === 404 && missing !== undefined) return missing;
    if (res.status === 422 && race) throw new Race(await res.text());
    if (!res.ok) throw await this.error(res);
    return res.status === 204 ? null : res.json();
  }

  private async graphql(query: string, variables: Record<string, string>): Promise<Json> {
    const res = await this.request("POST", "https://api.github.com/graphql", { query, variables });
    if (!res.ok) throw await this.error(res);
    const body: Json = await res.json();
    if (body.errors?.length) throw new GitError("GitHub", 502, body.errors[0].message);
    return body.data;
  }

  private async error(res: Response): Promise<GitError> {
    let message = `HTTP ${res.status}`;
    try {
      message = ((await res.json()) as Json).message ?? message;
    } catch {}
    if (res.status === 401) message = `the token was refused (${message})`;
    if (res.status === 403 || res.status === 404) message = `no access to ${this.repo} (${message})`;
    return new GitError("GitHub", res.status, message);
  }

  private toRepo(path: string): string {
    return this.dir + path;
  }

  /** A repository path as a project path; outside the project: `/path` (never writable). */
  private fromRepo(path: string): string {
    return path.startsWith(this.dir) ? path.slice(this.dir.length) : `/${path}`;
  }

  async head(branch: string): Promise<string | null> {
    const ref = await this.api("GET", `/git/ref/heads/${encodeSegments(branch)}`, undefined, { missing: null });
    return ref?.object?.sha ?? null;
  }

  /** The branches under `prefix` with their last commit, newest first. */
  async branchesWithHeads(prefix: string) {
    const data = await this.graphql(
      `query($owner: String!, $name: String!, $prefix: String!) {
        repository(owner: $owner, name: $name) {
          refs(refPrefix: $prefix, first: 100, orderBy: {field: TAG_COMMIT_DATE, direction: DESC}) {
            nodes { name target { ... on Commit { oid message committedDate author { name email } } } }
          }
        }
      }`,
      { owner: this.owner, name: this.name, prefix: `refs/heads/${prefix}` },
    );
    const nodes: Json[] = data.repository?.refs?.nodes ?? [];
    return nodes.map((n) => ({
      name: String(n.name),
      sha: n.target?.oid as string | undefined,
      message: String(n.target?.message ?? ""),
      date: n.target?.committedDate as string | undefined,
      author: n.target?.author ? ({ name: n.target.author.name, email: n.target.author.email } as Author) : null,
    }));
  }

  /** A file at `ref` as `{sha, size, content (base64)}`, or null. */
  async read(ref: string, path: string): Promise<{ sha: string; size: number; content: string } | null> {
    const r = await this.api(
      "GET",
      `/contents/${encodeSegments(this.toRepo(path))}?ref=${encodeURIComponent(ref)}`,
      undefined,
      { missing: null },
    );
    if (r === null) return null;
    if (Array.isArray(r) || r.type !== "file") throw new HttpError(400, `${path} is not a file`);
    let content: string = r.content ?? "";
    if (!content && r.size > 0) content = (await this.api("GET", `/git/blobs/${r.sha}`)).content;
    return { sha: r.sha, size: r.size, content: content.replace(/\s+/g, "") };
  }

  /** The blob ids of `paths` at commit `sha` (`null`: no such file). */
  async blobIds(sha: string, paths: string[]): Promise<Record<string, string | null>> {
    if (paths.length === 0) return {};
    const vars: Record<string, string> = {};
    const fields = paths.map((p, i) => {
      vars[`e${i}`] = `${sha}:${this.toRepo(p)}`;
      return `f${i}: object(expression: $e${i}) { oid }`;
    });
    const data = await this.graphql(
      `query($owner: String!, $name: String!, ${paths.map((_, i) => `$e${i}: String!`).join(", ")}) {
        repository(owner: $owner, name: $name) { ${fields.join(" ")} }
      }`,
      { owner: this.owner, name: this.name, ...vars },
    );
    const out: Record<string, string | null> = {};
    paths.forEach((p, i) => {
      out[p] = data.repository?.[`f${i}`]?.oid ?? null;
    });
    return out;
  }

  /** Commits `changes` to `branch`, made from branch `from` when it does not exist yet. A change
   * whose `base` (the blob id the editor started from; null: a new file) is no longer the
   * file's fails with 409, as does a move whose source is gone; a moved file keeps its blob. */
  async commitFiles(
    branch: string,
    from: string | null,
    changes: Change[],
    message: string,
    author: Author,
    now: () => number,
  ): Promise<string> {
    const blobs = new Map<string, string>();
    for (const c of changes) {
      if (!c.delete && c.encoding === "base64") {
        const r = await this.api("POST", "/git/blobs", `{"content":${JSON.stringify(c.content)},"encoding":"base64"}`);
        blobs.set(c.path, r.sha);
      }
    }
    const moves = changes.filter((c) => c.from !== undefined);
    const entries = changes.filter((c) => c.from === undefined).map((c): TreeEntry => {
      if (c.delete) return { path: c.path, sha: null };
      const blob = blobs.get(c.path);
      return blob ? { path: c.path, sha: blob } : { path: c.path, content: c.content };
    });
    for (let attempt = 0; ; attempt++) {
      let parent = await this.head(branch);
      const creating = parent === null;
      if (parent === null) {
        if (!from) throw new HttpError(500, `branch ${branch} does not exist`);
        parent = await this.head(from);
        if (!parent) throw new HttpError(500, `branch ${from} does not exist`);
      }
      const checks = changes.filter((c) => c.base !== undefined);
      const current = await this.blobIds(
        parent,
        checks.map((c) => c.path),
      );
      const stale = checks.filter((c) => (current[c.path] ?? null) !== c.base).map((c) => c.path);
      if (stale.length > 0) {
        throw new HttpError(409, "someone changed these files since you opened them; reload them first", { stale });
      }
      const sources = moves.length > 0 ? await this.blobIds(parent, moves.map((c) => c.from as string)) : {};
      const gone = moves.map((c) => c.from as string).filter((f) => !sources[f]);
      if (gone.length > 0) {
        throw new HttpError(409, "these files are gone since you opened the page; reload it first", { stale: gone });
      }
      const moved = moves.map((c): TreeEntry => ({ path: c.path, sha: sources[c.from as string] as string }));
      try {
        return await this.commitOnto(branch, parent, creating, entries.concat(moved), message, author, now);
      } catch (e) {
        if (e instanceof Race && attempt < 2) continue;
        if (e instanceof Race) throw new HttpError(409, `branch ${branch} keeps changing; try again`);
        throw e;
      }
    }
  }

  /** Commits tree `entries` on top of `parent` and moves `branch` to it (fails with `Race` when
   * the branch moved). */
  async commitTree(
    branch: string,
    parent: string,
    entries: TreeEntry[],
    message: string,
    author: Author,
    now: () => number,
  ): Promise<string> {
    return this.commitOnto(branch, parent, false, entries, message, author, now);
  }

  private async commitOnto(
    branch: string,
    parent: string,
    creating: boolean,
    entries: TreeEntry[],
    message: string,
    author: Author,
    now: () => number,
  ): Promise<string> {
    const base = await this.api("GET", `/git/commits/${parent}`);
    const tree = await this.api("POST", "/git/trees", {
      base_tree: base.tree.sha,
      tree: entries.map((e) => ({
        path: this.toRepo(e.path),
        mode: "100644",
        type: "blob",
        ...(e.content !== undefined ? { content: e.content } : { sha: e.sha }),
      })),
    });
    const commit = await this.api("POST", "/git/commits", {
      message,
      tree: tree.sha,
      parents: [parent],
      author: { ...author, date: new Date(now()).toISOString() },
    });
    if (creating) {
      await this.api("POST", "/git/refs", { ref: `refs/heads/${branch}`, sha: commit.sha }, { race: true });
    } else {
      await this.api(
        "PATCH",
        `/git/refs/heads/${encodeSegments(branch)}`,
        { sha: commit.sha, force: false },
        { race: true },
      );
    }
    return commit.sha;
  }

  /** What `head` changed since its merge base with `base`. */
  async compare(base: string, head: string): Promise<Comparison> {
    const r = await this.api("GET", `/compare/${base}...${head}`);
    const files: Json[] = r.files ?? [];
    const commits: Json[] = r.commits ?? [];
    return {
      mergeBase: r.merge_base_commit?.sha,
      files: files.map((f) => ({
        path: this.fromRepo(f.filename),
        status: f.status,
        sha: f.sha,
        previous: f.previous_filename ? this.fromRepo(f.previous_filename) : undefined,
        patch: f.patch,
      })),
      commits: commits.map((c) => ({
        sha: c.sha,
        author: c.commit?.author ? { name: c.commit.author.name, email: c.commit.author.email } : null,
        message: c.commit?.message ?? "",
      })),
    };
  }

  async deleteBranch(branch: string): Promise<void> {
    await this.api("DELETE", `/git/refs/heads/${encodeSegments(branch)}`, undefined, { missing: null });
  }
}
