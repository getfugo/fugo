// The Worker's settings, bindings and data, its limits, its errors and its JSON answers.

import { type Limits } from "../common";

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
  login: AccessLogin | { kind: "oauth"; providers: OauthProvider[] };
  roles: Record<string, Role>;
  maxUpload: number;
}

/** Cloudflare Access signs people in (`team`: the tokens' issuer; `aud`: the applications'). */
export interface AccessLogin {
  kind: "cloudflare-access";
  team: string;
  aud: string[];
}

/** An account the Worker signs people in with. */
export type OauthProvider = "github" | "google";

/** The Worker's bindings and secrets. */
export interface Env {
  ASSETS?: { fetch(request: Request): Promise<Response> };
  CMS_USERS?: string;
  CMS_GITHUB_TOKEN?: string;
  CMS_GITHUB_APP_ID?: string;
  CMS_GITHUB_APP_KEY?: string;
  CMS_SESSION_KEY?: string;
  CMS_GITHUB_CLIENT_ID?: string;
  CMS_GITHUB_CLIENT_SECRET?: string;
  CMS_GOOGLE_CLIENT_ID?: string;
  CMS_GOOGLE_CLIENT_SECRET?: string;
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

export interface User {
  email: string;
  name: string;
  roles: string[];
  edit: string[];
  publish: boolean;
}

export interface Author {
  name: string;
  email: string;
}

/** A checked change of a save: a file's new content, or its deletion. `base`: the blob id the
 * editor started from (`null`: a new file; `undefined`: not checked). */
export interface Change {
  path: string;
  base: string | null | undefined;
  delete?: true;
  content?: string;
  encoding?: "base64" | "utf-8";
  /** A move: the file is the one at `from`, which the same save deletes. */
  from?: string;
}

/** A tree entry: a blob (`sha`, null deletes) or content. */
export interface TreeEntry {
  path: string;
  sha?: string | null;
  content?: string;
}

export interface CompareFile {
  path: string;
  status: string;
  sha: string;
  previous?: string;
  patch?: string;
}

/** A draft's open pull request. */
export interface Pull {
  number: number;
  url: string;
  title: string;
}

export interface Comparison {
  mergeBase: string;
  files: CompareFile[];
  /** `merge`: a merge commit (one that brought the branch into a draft). */
  commits: { sha: string; author: Author | null; message: string; merge: boolean }[];
}

/** A JSON answer of an API this module does not describe (GitHub's, read field by field). */
export type Json = any;

export type Body = Record<string, unknown>;

export const DRAFTS = "cms/";
/** The label of the drafts' pull requests. */
export const LABEL = "fugo-cms";
export const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]"]);
export const MAX_CHANGES = 30;
export const MAX_PATCH = 20000;
export const USER_AGENT = "cms-worker";

/** An error with the HTTP status the API answers with. */
export class HttpError extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly extra: Record<string, unknown> = {},
  ) {
    super(message);
  }
}

/** The branch moved while a commit was made (retried). */
export class Race extends Error {}

/** An error answer of the git host. */
export class GitError extends Error {
  constructor(
    readonly host: string,
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

const b64url = (s: string) => s.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(s.length / 4) * 4, "=");
export const b64urlBytes = (s: string) => Uint8Array.from(atob(b64url(s)), (c) => c.charCodeAt(0));
export const b64urlJson = (s: string): Json => JSON.parse(new TextDecoder().decode(b64urlBytes(s)));
export function toB64url(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/=+$/, "").replace(/\+/g, "-").replace(/\//g, "_");
}

export const JSON_HEADERS = {
  "content-type": "application/json; charset=utf-8",
  "cache-control": "no-store",
  "x-content-type-options": "nosniff",
};
export const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: JSON_HEADERS });
