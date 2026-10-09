// The API of the CMS editor, as a Cloudflare Worker module. The site's build publishes it as
// `_worker.js`: the `[cms]` settings and the content index as two constants
// (`__CMS_SETTINGS__`, `__CMS_INDEX__`), then this module, bundled (with `common.ts` and
// `worker/`) and minified. Its default export, the Worker, reads them on the first request.
//
// Requests under `SETTINGS.api` (`/admin/api/`) are the API; anything else goes to the static
// files (`env.ASSETS`), for a Worker that runs first on every path.
//
// Sign-in (`[cms.login]`): the Worker signs people in itself with a GitHub or Google account and
// keeps them signed in with a cookie it signs (worker/oauth.ts); or Cloudflare Access signs every
// request in, and the Worker checks the token Access adds (`Cf-Access-Jwt-Assertion`) against the
// team's keys and the application's audience, so a request that bypasses Access is refused too.
// For `wrangler dev` only, a request to localhost that is not signed in is `CMS_DEV_USER` (a
// variable of `.dev.vars`, which is never deployed). The `CMS_USERS` secret gives an email its roles (`{"ann@example.com": ["writer"],
// "@example.com": ["reader"]}`); a role's `edit` globs are cut down to the settings' areas
// (`mayEdit`). Commits go to the git host as one bot (`CMS_GITHUB_TOKEN`, or the GitHub App of
// `CMS_GITHUB_APP_ID` and `CMS_GITHUB_APP_KEY`), with the editor as author.
//
// Workflow `review`: a save commits to the draft branch of its page (`cms/<id>`, made from the
// branch, with a pull request labelled `fugo-cms` when the credential may open one); publishing
// merges the branch into a draft whose files it changed since, then copies the draft's files
// onto the branch in one commit and deletes the draft. Decap CMS's drafts
// (`cms/<collection>/<slug>`) are drafts too. Workflow `direct`: a save commits to the branch.

import { AccessKeys } from "./worker/access";
import { Api } from "./worker/api";
import { appToken, type TokenCache } from "./worker/app";
import { GitHub } from "./worker/github";
import { OAuth } from "./worker/oauth";
import { type Body, type Env, type Fetch, GitError, HttpError, json, JSON_HEADERS, LOCAL_HOSTS, type Settings, type User, type WorkerOptions } from "./worker/shared";

export * from "./common";
export { draftId } from "./worker/api";
export { pkcs1ToPkcs8 } from "./worker/app";
export { GitHub } from "./worker/github";
export type { Env, Fetch, Role, Settings, WorkerOptions } from "./worker/shared";

/** The Worker of `settings`. */
export function createWorker(settings: Settings, options: WorkerOptions = {}) {
  const fetcher: Fetch = options.fetch ?? ((input, init) => fetch(input, init));
  const now = options.now ?? (() => Date.now());
  const index = options.index;
  const login = settings.login;
  const access = login.kind === "cloudflare-access" ? new AccessKeys(login, fetcher, now) : null;
  const oauth = login.kind === "oauth" ? new OAuth(settings, login.providers, fetcher, now) : null;
  const appTokens: TokenCache = new Map();
  let users: { raw: string | undefined; map: Map<string, string[]> } = { raw: undefined, map: new Map() };

  async function signIn(request: Request, url: URL, env: Env): Promise<User> {
    const token = request.headers.get("cf-access-jwt-assertion");
    let who: { email: string; name?: string } | null = null;
    if (access && token) {
      who = { email: await access.verify(token) };
    } else if (oauth) {
      who = await oauth.session(request, env);
    }
    if (!who && env.CMS_DEV_USER && LOCAL_HOSTS.has(url.hostname)) {
      who = { email: String(env.CMS_DEV_USER).trim().toLowerCase() };
    }
    if (!who) throw oauth?.notSignedIn() ?? new HttpError(401, "not signed in: open the editor through Cloudflare Access");
    const { email } = who;
    const roles = rolesOf(email, env);
    if (roles.length === 0) throw new HttpError(403, `${email} has no role in the editor`);
    return {
      email,
      name: who.name || email.slice(0, email.indexOf("@")) || email,
      roles,
      edit: [...new Set(roles.flatMap((r) => settings.roles[r].edit))],
      publish: roles.some((r) => settings.roles[r].publish),
    };
  }

  /** The roles `CMS_USERS` gives an email (lower case), by the email or its domain. */
  function rolesOf(email: string, env: Env): string[] {
    const raw = env.CMS_USERS;
    if (!raw) throw new HttpError(500, "the CMS_USERS secret is not set");
    if (users.raw !== raw) users = { raw, map: parseUsers(raw) };
    const names = users.map.get(email) ?? users.map.get(email.slice(email.indexOf("@"))) ?? [];
    return names.filter((n) => Object.hasOwn(settings.roles, n));
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
    if (oauth) {
      // Signing in and out: before (and without) a sign-in.
      const at = /^(login|callback)\/([^/]*)$/.exec(name);
      if (method === "GET" && at?.[1] === "login") return oauth.start(url, env, at[2]);
      if (method === "GET" && at?.[1] === "callback") return oauth.callback(request, url, env, at[2], (e) => rolesOf(e, env).length > 0);
      if (method === "POST" && name === "logout") return oauth.signOut();
    }
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

// ── Git hosts ──────────────────────────────────────────────────────────────────────────────────
//
// A host has the methods of `GitHub` (worker/github.ts): head, branchesWithHeads, read, blobIds,
// commitFiles, commitTree, compare, mergeInto, deleteBranch, openPulls, openPull, comment. Paths
// are project-relative; the host adds `git.dir`.

