// The API of the CMS editor, as a Cloudflare Worker module. The site's build publishes it as
// `_worker.js`: the `[cms]` settings and the content index as two constants
// (`__CMS_SETTINGS__`, `__CMS_INDEX__`), then this module, bundled (with `common.ts`) and
// minified. Its default export, the Worker, reads them on the first request.
//
// Requests under `SETTINGS.api` (`/admin/api/`) are the API; anything else goes to the static
// files (`env.ASSETS`), for a Worker that runs first on every path.
//
// Every request is signed in by Cloudflare Access: the Worker checks the token Access adds
// (`Cf-Access-Jwt-Assertion`) against the team's keys and the application's audience, so a
// request that bypasses Access is refused too. For `wrangler dev` only, a request to localhost
// without a token is `CMS_DEV_USER` (a variable of `.dev.vars`, which is never deployed). The
// `CMS_USERS` secret gives an email its roles (`{"ann@example.com": ["writer"],
// "@example.com": ["reader"]}`); a role's `edit` globs are cut down to the settings' areas
// (`mayEdit`). Commits go to the git host as one bot (`CMS_GITHUB_TOKEN`, or the GitHub App of
// `CMS_GITHUB_APP_ID` and `CMS_GITHUB_APP_KEY`), with the editor as author.
//
// Workflow `review`: a save commits to the draft branch of its page (`cms/<id>`, made from the
// branch); publishing copies the draft's files onto the branch in one commit and deletes the
// draft. Workflow `direct`: a save commits to the branch.

import { type Limits, areaOf, base64Size, cleanPath, commitMessage, mayEdit, parseTrailers } from "./common";

export * from "./common";

/** What a role may do. */
export interface Role {
  edit: string[];
  publish: boolean;
}

/** The `[cms]` settings the build publishes the Worker with (`crates/cms/src/lib.rs`). */
export interface Settings extends Limits {
  version: number;
  /** The editor's URL path (`/admin/`). */
  path: string;
  /** The API's URL path (`/admin/api/`). */
  api: string;
  site: string;
  workflow: "review" | "direct";
  git: { host: string; repo: string; branch: string; dir?: string };
  login: { provider: string; team: string; aud: string[] };
  roles: Record<string, Role>;
  maxUpload: number;
}

/** The Worker's bindings and secrets. */
export interface Env {
  ASSETS?: { fetch(request: Request): Promise<Response> };
  CMS_USERS?: string;
  CMS_GITHUB_TOKEN?: string;
  CMS_GITHUB_APP_ID?: string;
  CMS_GITHUB_APP_KEY?: string;
  CMS_DEV_USER?: string;
}

export type Fetch = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;

export interface WorkerOptions {
  /** The content index (`GET site`), as JSON text. */
  index?: string;
  /** `fetch` and the clock (tests). */
  fetch?: Fetch;
  now?: () => number;
}

interface User {
  email: string;
  name: string;
  roles: string[];
  edit: string[];
  publish: boolean;
}

interface Author {
  name: string;
  email: string;
}

/** A checked change of a save: a file's new content, or its deletion. `base`: the blob id the
 * editor started from (`null`: a new file; `undefined`: not checked). */
interface Change {
  path: string;
  base: string | null | undefined;
  delete?: true;
  content?: string;
  encoding?: "base64" | "utf-8";
}

/** A tree entry: a blob (`sha`, null deletes) or content. */
interface TreeEntry {
  path: string;
  sha?: string | null;
  content?: string;
}

interface CompareFile {
  path: string;
  status: string;
  sha: string;
  previous?: string;
  patch?: string;
}

interface Comparison {
  mergeBase: string;
  files: CompareFile[];
  commits: { sha: string; author: Author | null; message: string }[];
}

/** A JSON answer of an API this module does not describe (GitHub's, read field by field). */
type Json = any;

type Body = Record<string, unknown>;

const DRAFTS = "cms/";
const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]"]);
const MAX_CHANGES = 30;
const MAX_PATCH = 20000;
const USER_AGENT = "cms-worker";

/** An error with the HTTP status the API answers with. */
class HttpError extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly extra: Record<string, unknown> = {},
  ) {
    super(message);
  }
}

/** The branch moved while a commit was made (retried). */
class Race extends Error {}

/** An error answer of the git host. */
class GitError extends Error {
  constructor(
    readonly host: string,
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

const JSON_HEADERS = {
  "content-type": "application/json; charset=utf-8",
  "cache-control": "no-store",
  "x-content-type-options": "nosniff",
};
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: JSON_HEADERS });

/** The Worker of `settings`. */
export function createWorker(settings: Settings, options: WorkerOptions = {}) {
  const fetcher: Fetch = options.fetch ?? ((input, init) => fetch(input, init));
  const now = options.now ?? (() => Date.now());
  const index = options.index;
  const access = new AccessKeys(settings.login, fetcher, now);
  const appTokens: TokenCache = new Map();
  let users: { raw: string | undefined; map: Map<string, string[]> } = { raw: undefined, map: new Map() };

  async function signIn(request: Request, url: URL, env: Env): Promise<User> {
    const token = request.headers.get("cf-access-jwt-assertion");
    let email: string;
    if (token) {
      email = await access.verify(token);
    } else if (env.CMS_DEV_USER && LOCAL_HOSTS.has(url.hostname)) {
      email = String(env.CMS_DEV_USER).trim().toLowerCase();
    } else {
      throw new HttpError(401, "not signed in: open the editor through Cloudflare Access");
    }
    const raw = env.CMS_USERS;
    if (!raw) throw new HttpError(500, "the CMS_USERS secret is not set");
    if (users.raw !== raw) users = { raw, map: parseUsers(raw) };
    const names = users.map.get(email) ?? users.map.get(email.slice(email.indexOf("@"))) ?? [];
    const roles = names.filter((n) => Object.hasOwn(settings.roles, n));
    if (roles.length === 0) throw new HttpError(403, `${email} has no role in the editor`);
    return {
      email,
      name: email.slice(0, email.indexOf("@")) || email,
      roles,
      edit: [...new Set(roles.flatMap((r) => settings.roles[r].edit))],
      publish: roles.some((r) => settings.roles[r].publish),
    };
  }

  function host(env: Env): GitHub {
    const git = settings.git;
    if (git.host !== "github") throw new HttpError(500, `git host ${git.host} is not supported`);
    const token = async (): Promise<string> => {
      if (env.CMS_GITHUB_TOKEN) return env.CMS_GITHUB_TOKEN;
      if (env.CMS_GITHUB_APP_ID && env.CMS_GITHUB_APP_KEY) {
        return appToken(appTokens, env.CMS_GITHUB_APP_ID, env.CMS_GITHUB_APP_KEY, git.repo, fetcher, now);
      }
      throw new HttpError(500, "set the CMS_GITHUB_TOKEN secret, or CMS_GITHUB_APP_ID and CMS_GITHUB_APP_KEY");
    };
    return new GitHub(git, token, fetcher);
  }

  async function route(request: Request, url: URL, env: Env): Promise<unknown> {
    const method = request.method;
    const name = url.pathname.slice(settings.api.length);
    if (method !== "GET" && method !== "POST") throw new HttpError(405, "method not allowed");
    if (method === "POST") checkPost(request, url, settings);
    const user = await signIn(request, url, env);
    // The body only after sign-in: nobody else gets the Worker to read or parse anything.
    const body = method === "POST" ? await readBody(request, settings) : {};
    if (method === "GET" && name === "site") {
      if (index === undefined) throw new HttpError(404, "no content index");
      return new Response(index, { headers: JSON_HEADERS });
    }
    const api = new Api(settings, host(env), user, now);
    switch (`${method} ${name}`) {
      case "GET me":
        return api.me();
      case "GET file":
        return api.file(url.searchParams.get("path"), url.searchParams.get("draft"));
      case "GET drafts":
        return api.drafts();
      case "GET draft":
        return api.draft(url.searchParams.get("id"));
      case "POST save":
        return api.save(body);
      case "POST publish":
        return api.publish(body);
      case "POST discard":
        return api.discard(body);
      default:
        throw new HttpError(404, `no API ${method} ${name}`);
    }
  }

  return {
    async fetch(request: Request, env: Env = {}): Promise<Response> {
      const url = new URL(request.url);
      if (!url.pathname.startsWith(settings.api)) {
        return env.ASSETS ? env.ASSETS.fetch(request) : new Response("Not found", { status: 404 });
      }
      try {
        const out = await route(request, url, env);
        return out instanceof Response ? out : json(out);
      } catch (e) {
        if (e instanceof HttpError) return json({ error: e.message, ...e.extra }, e.status);
        if (e instanceof GitError) return json({ error: `${e.host}: ${e.message}` }, 502);
        return json({ error: `internal error: ${e instanceof Error ? e.message : String(e)}` }, 500);
      }
    },
  };
}

// The settings and the content index, which the build puts before this module in `_worker.js`
// (free names, so minifying leaves them alone).
declare const __CMS_SETTINGS__: Settings;
declare const __CMS_INDEX__: string;

let published: ReturnType<typeof createWorker> | undefined;

/** The Worker of `_worker.js`, made on its first request. */
export default {
  fetch(request: Request, env: Env): Promise<Response> {
    published ??= createWorker(__CMS_SETTINGS__, { index: __CMS_INDEX__ });
    return published.fetch(request, env);
  },
};

/** A POST is same-origin (no cross-site forms) JSON. */
function checkPost(request: Request, url: URL, settings: Settings): void {
  if (request.headers.get("origin") !== url.origin) throw new HttpError(403, "cross-site request");
  if (!(request.headers.get("content-type") ?? "").startsWith("application/json")) {
    throw new HttpError(415, "send JSON");
  }
  if (Number(request.headers.get("content-length") ?? 0) > bodyLimit(settings)) {
    throw new HttpError(413, "too large");
  }
}

/** The largest body: an upload as base64, and room for the rest. */
const bodyLimit = (settings: Settings) => Math.ceil(settings.maxUpload * 1.4) + (1 << 20);

/** A POST body as a JSON object, read up to the body limit (whatever its length header says). */
async function readBody(request: Request, settings: Settings): Promise<Body> {
  const limit = bodyLimit(settings);
  const chunks: Uint8Array[] = [];
  let size = 0;
  if (request.body) {
    const reader = request.body.getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) {
        await reader.cancel();
        throw new HttpError(413, "too large");
      }
      chunks.push(value);
    }
  }
  const bytes = new Uint8Array(size);
  let at = 0;
  for (const c of chunks) {
    bytes.set(c, at);
    at += c.byteLength;
  }
  const text = new TextDecoder().decode(bytes);
  try {
    const body: unknown = JSON.parse(text);
    if (body === null || typeof body !== "object" || Array.isArray(body)) throw new Error();
    return body as Body;
  } catch {
    throw new HttpError(400, "invalid JSON");
  }
}

/** `CMS_USERS`: a JSON object from emails (or `@domain`) to role names (a list, or one string
 * of comma-separated names); keys and names are compared in lower case. */
export function parseUsers(raw: string): Map<string, string[]> {
  let obj: unknown;
  try {
    obj = JSON.parse(raw);
  } catch {
    throw new HttpError(500, "the CMS_USERS secret is not JSON");
  }
  if (obj === null || typeof obj !== "object" || Array.isArray(obj)) {
    throw new HttpError(500, 'CMS_USERS is a JSON object: {"email": ["role"]}');
  }
  const map = new Map<string, string[]>();
  for (const [k, v] of Object.entries(obj)) {
    const names = (Array.isArray(v) ? v : String(v).split(","))
      .map((n) => String(n).trim().toLowerCase())
      .filter(Boolean);
    map.set(k.trim().toLowerCase(), names);
  }
  return map;
}

// ── Cloudflare Access ──────────────────────────────────────────────────────────────────────────

const b64url = (s: string) => s.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(s.length / 4) * 4, "=");
const b64urlBytes = (s: string) => Uint8Array.from(atob(b64url(s)), (c) => c.charCodeAt(0));
const b64urlJson = (s: string): Json => JSON.parse(new TextDecoder().decode(b64urlBytes(s)));

/** The public keys of an Access team, fetched when a token names a key not seen yet (at most
 * once a minute). */
class AccessKeys {
  private keys = new Map<string, CryptoKey>();
  private fetchedAt = -Infinity;

  constructor(
    private readonly login: Settings["login"],
    private readonly fetcher: Fetch,
    private readonly now: () => number,
  ) {}

  /** The email of a valid token (lower case). */
  async verify(token: string): Promise<string> {
    const invalid = (why: string) => new HttpError(401, `invalid sign-in token: ${why}`);
    const parts = token.split(".");
    if (parts.length !== 3) throw invalid("not a JWT");
    let header: Json;
    let claims: Json;
    try {
      header = b64urlJson(parts[0]);
      claims = b64urlJson(parts[1]);
    } catch {
      throw invalid("not a JWT");
    }
    if (header.alg !== "RS256") throw invalid(`algorithm ${header.alg}`);
    const key = await this.key(header.kid);
    const signed = new TextEncoder().encode(`${parts[0]}.${parts[1]}`);
    let ok = false;
    try {
      ok = await crypto.subtle.verify("RSASSA-PKCS1-v1_5", key, b64urlBytes(parts[2]), signed);
    } catch {
      ok = false;
    }
    if (!ok) throw invalid("bad signature");
    if (claims.iss !== this.login.team) throw invalid("issued by another team");
    const aud: unknown[] = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
    if (!aud.some((a) => typeof a === "string" && this.login.aud.includes(a))) {
      throw invalid("for another application");
    }
    const t = this.now() / 1000;
    if (typeof claims.exp !== "number" || claims.exp < t - 60) throw invalid("expired");
    if (typeof claims.nbf === "number" && claims.nbf > t + 60) throw invalid("not valid yet");
    if (typeof claims.email !== "string" || !claims.email.includes("@")) {
      throw new HttpError(403, "this sign-in has no email (service tokens cannot edit)");
    }
    return claims.email.trim().toLowerCase();
  }

  private async key(kid: unknown): Promise<CryptoKey> {
    const known = typeof kid === "string" ? this.keys.get(kid) : undefined;
    if (known) return known;
    if (typeof kid !== "string" || this.now() - this.fetchedAt < 60_000) {
      throw new HttpError(401, "invalid sign-in token: unknown key");
    }
    this.fetchedAt = this.now();
    const res = await this.fetcher(`${this.login.team}/cdn-cgi/access/certs`);
    if (!res.ok) throw new HttpError(502, `Cloudflare Access keys: HTTP ${res.status}`);
    const body: Json = await res.json();
    const keys = new Map<string, CryptoKey>();
    for (const jwk of body.keys ?? []) {
      if (jwk.kty !== "RSA" || !jwk.kid) continue;
      const key = await crypto.subtle.importKey(
        "jwk",
        { kty: "RSA", n: jwk.n, e: jwk.e, alg: "RS256", ext: true },
        { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
        false,
        ["verify"],
      );
      keys.set(jwk.kid, key);
    }
    this.keys = keys;
    const found = keys.get(kid);
    if (!found) throw new HttpError(401, "invalid sign-in token: unknown key");
    return found;
  }
}

// ── The API ────────────────────────────────────────────────────────────────────────────────────

class Api {
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

  /** The open drafts, newest first. */
  async drafts() {
    const refs = await this.git.branchesWithHeads(DRAFTS);
    return {
      drafts: refs
        .filter((r) => isDraftId(r.name))
        .map((r) => {
          const t = parseTrailers(r.message);
          return {
            id: r.name,
            entry: t["cms-entry"] ?? "",
            title: t["cms-title"] ?? "",
            author: r.author,
            updated: r.date,
          };
        }),
    };
  }

  /** A draft: its files (with their diff), its commits, and what conflicts with the branch. */
  async draft(id: unknown) {
    const head = await this.draftHead(id);
    const main = await this.mainHead();
    const cmp = await this.git.compare(main, head);
    const t = parseTrailers(cmp.commits.at(-1)?.message);
    return {
      id,
      entry: t["cms-entry"] ?? "",
      title: t["cms-title"] ?? "",
      files: cmp.files.map((f) => ({
        path: f.path,
        status: f.status,
        previous: f.previous,
        patch: f.patch && f.patch.length > MAX_PATCH ? `${f.patch.slice(0, MAX_PATCH)}\n…` : f.patch,
      })),
      commits: cmp.commits.map((c) => ({
        author: c.author,
        subject: c.message.split("\n")[0],
      })),
      conflicts: await this.conflicts(cmp, main),
    };
  }

  /** Commits changes to the draft of `entry` (or the branch, workflow `direct`). */
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
      if (typeof c.content !== "string") throw new HttpError(400, `no content for ${path}`);
      const encoding = c.encoding === "base64" ? "base64" : "utf-8";
      const size = encoding === "base64" ? base64Size(c.content) : new TextEncoder().encode(c.content).length;
      if (size > this.s.maxUpload) throw new HttpError(413, `${path} is larger than the upload limit`);
      if (encoding === "base64" && !/^[A-Za-z0-9+/\s]*={0,2}\s*$/.test(c.content)) {
        throw new HttpError(400, `${path} is not base64`);
      }
      return { path, base, content: c.content, encoding };
    });
    const title = typeof body.title === "string" ? body.title : "";
    const verb = checked.every((c) => c.delete) ? "Delete" : "Edit";
    const message = commitMessage(`${verb} ${entry}${title ? `: ${title}` : ""}`, [
      ["CMS-Entry", entry],
      ["CMS-Title", title],
    ]);
    const author = { name: this.user.name, email: this.user.email };
    if (this.s.workflow === "direct") {
      const commit = await this.git.commitFiles(this.s.git.branch, null, checked, message, author, this.now);
      return { commit, draft: null };
    }
    const id = await draftId(entry);
    const commit = await this.git.commitFiles(DRAFTS + id, this.s.git.branch, checked, message, author, this.now);
    return { commit, draft: id };
  }

  /** Copies a draft's files onto the branch in one commit, then deletes the draft. */
  async publish(body: Body) {
    if (this.s.workflow !== "review") throw new HttpError(400, "there are no drafts: workflow is direct");
    if (!this.user.publish) throw new HttpError(403, "you may not publish");
    const id = body.id;
    for (let attempt = 0; ; attempt++) {
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
      const conflicts = await this.conflicts(cmp, main);
      if (conflicts.length > 0) {
        throw new HttpError(409, "the branch changed these files since the draft was made", { conflicts });
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
      const t = parseTrailers(cmp.commits.at(-1)?.message);
      const authors: Author[] = [];
      for (const c of cmp.commits) {
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
        await this.git.deleteBranch(DRAFTS + id);
        return { commit, published: true };
      } catch (e) {
        if (e instanceof Race && attempt < 2) continue;
        if (e instanceof Race) throw new HttpError(409, "the branch keeps changing; try again");
        throw e;
      }
    }
  }

  /** Deletes a draft: a publisher's, or one whose commits are all the user's. */
  async discard(body: Body) {
    const id = body.id;
    const head = await this.draftHead(id);
    if (!this.user.publish) {
      const cmp = await this.git.compare(await this.mainHead(), head);
      if (cmp.commits.some((c) => (c.author?.email ?? "").toLowerCase() !== this.user.email)) {
        throw new HttpError(403, "others edited this draft: ask someone who may publish to discard it");
      }
    }
    await this.git.deleteBranch(DRAFTS + id);
    return { discarded: id };
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

  /** The draft's paths the branch changed since the draft was made from it: their blob at the
   * merge base and on the branch differ (exact, however many files the branch changed). */
  private async conflicts(cmp: Comparison, main: string): Promise<string[]> {
    if (cmp.mergeBase === main) return [];
    const paths = [...new Set(cmp.files.flatMap(pathsOf))];
    const [then, nowIds] = await Promise.all([this.git.blobIds(cmp.mergeBase, paths), this.git.blobIds(main, paths)]);
    return paths.filter((p) => then[p] !== nowIds[p]);
  }
}

/** A changed file's path, and its previous path when it moved. */
const pathsOf = (f: CompareFile): string[] => (f.previous ? [f.path, f.previous] : [f.path]);

const isDraftId = (id: unknown): id is string => typeof id === "string" && /^[a-z0-9][a-z0-9-]{0,79}$/.test(id);

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

// ── Git hosts ──────────────────────────────────────────────────────────────────────────────────
//
// A host has the methods of `GitHub` below: head, branchesWithHeads, read, blobIds,
// commitFiles, commitTree, compare, deleteBranch. Paths are project-relative; the host adds
// `git.dir`.

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
   * file's fails with 409. */
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
    const entries = changes.map((c): TreeEntry => {
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
      try {
        return await this.commitOnto(branch, parent, creating, entries, message, author, now);
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

// ── GitHub App tokens ──────────────────────────────────────────────────────────────────────────

type TokenCache = Map<string, { token: string; expires: number }>;

/** An installation token of the GitHub App for `repo`, cached until 5 minutes before it
 * expires. */
async function appToken(
  cache: TokenCache,
  appId: string,
  pem: string,
  repo: string,
  fetcher: Fetch,
  now: () => number,
): Promise<string> {
  const cached = cache.get(repo);
  if (cached && cached.expires - now() > 5 * 60_000) return cached.token;
  const key = await importAppKey(pem);
  const iat = Math.floor(now() / 1000) - 60;
  const jwt = await signJwt({ alg: "RS256", typ: "JWT" }, { iat, exp: iat + 540, iss: String(appId).trim() }, key);
  const call = async (method: string, path: string, body?: unknown): Promise<Json> => {
    const res = await fetcher(`https://api.github.com${path}`, {
      method,
      headers: {
        authorization: `Bearer ${jwt}`,
        accept: "application/vnd.github+json",
        "x-github-api-version": "2022-11-28",
        "user-agent": USER_AGENT,
        ...(body ? { "content-type": "application/json" } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    if (!res.ok) {
      let message = `HTTP ${res.status}`;
      try {
        message = ((await res.json()) as Json).message ?? message;
      } catch {}
      throw new GitError("GitHub App", res.status, `${message} (is the app installed on ${repo}?)`);
    }
    return res.json();
  };
  const installation = await call("GET", `/repos/${repo}/installation`);
  const token = await call("POST", `/app/installations/${installation.id}/access_tokens`, {
    repositories: [repo.split("/")[1]],
    permissions: { contents: "write" },
  });
  cache.set(repo, { token: token.token, expires: Date.parse(token.expires_at) });
  return token.token;
}

/** An RSA private key from PEM: PKCS#8 (`BEGIN PRIVATE KEY`) or PKCS#1 (`BEGIN RSA PRIVATE KEY`,
 * what GitHub hands out). Literal `\n` in the secret count as line breaks. */
async function importAppKey(pem: string): Promise<CryptoKey> {
  const text = pem.replace(/\\n/g, "\n");
  const body = text.replace(/-----[^-]+-----/g, "").replace(/\s+/g, "");
  const der = Uint8Array.from(atob(body), (c) => c.charCodeAt(0));
  const pkcs8 = /BEGIN RSA PRIVATE KEY/.test(text) ? pkcs1ToPkcs8(der) : der;
  return crypto.subtle.importKey("pkcs8", pkcs8, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["sign"]);
}

function derLength(n: number): number[] {
  if (n < 0x80) return [n];
  const bytes: number[] = [];
  for (; n > 0; n >>= 8) bytes.unshift(n & 0xff);
  return [0x80 | bytes.length, ...bytes];
}

/** Wraps a PKCS#1 RSAPrivateKey in a PKCS#8 PrivateKeyInfo (rsaEncryption, NULL parameters). */
export function pkcs1ToPkcs8(pkcs1: Uint8Array): Uint8Array<ArrayBuffer> {
  const algorithm = [0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00];
  const inner = [0x02, 0x01, 0x00, ...algorithm, 0x04, ...derLength(pkcs1.length)];
  const head = [0x30, ...derLength(inner.length + pkcs1.length), ...inner];
  const out = new Uint8Array(head.length + pkcs1.length);
  out.set(head, 0);
  out.set(pkcs1, head.length);
  return out;
}

async function signJwt(header: object, claims: object, key: CryptoKey): Promise<string> {
  const enc = (o: object) => btoa(JSON.stringify(o)).replace(/=+$/, "").replace(/\+/g, "-").replace(/\//g, "_");
  const input = `${enc(header)}.${enc(claims)}`;
  const sig = new Uint8Array(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, new TextEncoder().encode(input)));
  let s = "";
  for (const b of sig) s += String.fromCharCode(b);
  return `${input}.${btoa(s).replace(/=+$/, "").replace(/\+/g, "-").replace(/\//g, "_")}`;
}
