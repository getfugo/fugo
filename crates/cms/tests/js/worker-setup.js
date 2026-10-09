// The fixtures of the Worker tests: settings, files and users, Access keys and tokens, and a
// Worker over a fake GitHub, a fake Access team and fake providers to sign in with (`setup`).

import { createWorker } from "../../assets/worker.js";
import { FakeGitHub } from "./fake-github.js";
import { CLIENTS, FakeOAuth } from "./fake-oauth.js";

export const ORIGIN = "https://snack.example";
export const TEAM = "https://team.cloudflareaccess.com";
export const AUD = "aud-1";
export const NOW = Date.parse("2026-06-01T12:00:00Z");

export const SETTINGS = {
  version: 1,
  path: "/admin/",
  api: "/admin/api/",
  site: `${ORIGIN}/`,
  workflow: "review",
  git: { host: "github", repo: "owner/site", branch: "main", dir: "" },
  login: { kind: "cloudflare-access", team: TEAM, aud: [AUD] },
  roles: {
    writer: { edit: ["content/**/index.en.md", "content/**/*.{jpg,png,svg}", "content/blog/**"], publish: false },
    translator: { edit: ["content/**/index.th.md"], publish: false },
    publisher: { edit: ["**"], publish: true },
  },
  areas: [
    { kind: "content", glob: "content/**", ext: ["md", "jpg", "png"] },
    { kind: "data", glob: "data/**", ext: ["yaml", "toml", "json"] },
    { kind: "config", glob: "config/_default/{params,menus,menu}.*", ext: ["toml", "yaml", "yml", "json"] },
  ],
  deny: ["**/_content.*"],
  maxUpload: 64 * 1024,
};

/** The settings of a Worker that signs people in itself. */
export const OAUTH_SETTINGS = { ...SETTINGS, login: { kind: "oauth", providers: ["github", "google"] } };

export const FILES = {
  "content/almonds/honey/index.en.md": "---\ntitle: Honey\n---\nEnglish\n",
  "content/almonds/honey/index.th.md": "---\ntitle: น้ำผึ้ง\n---\nไทย\n",
  "content/blog/post.md": "---\ntitle: Post\n---\n",
  "content/books/_content.html": "{# adapter #}",
  "content/big.md": `---\ntitle: Big\n---\n${"x".repeat(3000)}`,
  "config.toml": "baseURL = 'x'\n",
  "layouts/home.html": "<p>x</p>\n",
  "data/snacks.yaml": "a: 1\n",
};

export const USERS = {
  "writer@example.com": ["writer"],
  "writer2@example.com": ["writer"],
  "Translator@Example.com": "translator",
  "boss@example.com": ["publisher"],
  "@team.example": ["writer"],
  "ghost@example.com": ["no-such-role"],
};

// ── Access keys and tokens ──

export const keys = await crypto.subtle.generateKey(
  { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
  true,
  ["sign", "verify"],
);
export const otherKeys = await crypto.subtle.generateKey(
  { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
  true,
  ["sign", "verify"],
);
export const publicJwk = { ...(await crypto.subtle.exportKey("jwk", keys.publicKey)), kid: "k1", alg: "RS256", use: "sig" };

export const b64url = (bytes) => Buffer.from(bytes).toString("base64url");
export async function token(claims = {}, { key = keys.privateKey, kid = "k1", alg = "RS256" } = {}) {
  const header = b64url(JSON.stringify({ alg, kid, typ: "JWT" }));
  const body = b64url(
    JSON.stringify({ iss: TEAM, aud: [AUD], exp: NOW / 1000 + 3600, iat: NOW / 1000, email: "writer@example.com", ...claims }),
  );
  const sig = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, new TextEncoder().encode(`${header}.${body}`));
  return `${header}.${body}.${b64url(new Uint8Array(sig))}`;
}

// ── A Worker over the fakes ──

export function setup({ settings = SETTINGS, files = FILES, env = {}, index, now = () => NOW } = {}) {
  const gh = new FakeGitHub({ files });
  const oauth = new FakeOAuth(now);
  let certFetches = 0;
  const fetch = async (input, init) => {
    const url = typeof input === "string" ? input : input.url;
    if (url === `${TEAM}/cdn-cgi/access/certs`) {
      certFetches++;
      return Response.json({ keys: [publicJwk], public_cert: { kid: "k1" } });
    }
    if (oauth.handles(url)) return oauth.fetch(url, init);
    return gh.fetch(input, init);
  };
  const worker = createWorker(settings, { fetch, now, index });
  const fullEnv = {
    CMS_USERS: JSON.stringify(USERS),
    CMS_GITHUB_TOKEN: "test-token",
    CMS_SESSION_KEY: "a session key of thirty-two characters or more",
    CMS_GITHUB_CLIENT_ID: CLIENTS.github.id,
    CMS_GITHUB_CLIENT_SECRET: CLIENTS.github.secret,
    CMS_GOOGLE_CLIENT_ID: CLIENTS.google.id,
    CMS_GOOGLE_CLIENT_SECRET: CLIENTS.google.secret,
    ...env,
  };
  async function call(method, name, { as = "writer@example.com", body, query, headers, jwt } = {}) {
    const url = new URL(`${settings.api}${name}`, ORIGIN);
    for (const [k, v] of Object.entries(query ?? {})) url.searchParams.set(k, v);
    const h = new Headers(headers ?? {});
    if (jwt !== null) h.set("cf-access-jwt-assertion", jwt ?? (await token({ email: as })));
    if (method === "POST") {
      if (!h.has("origin")) h.set("origin", ORIGIN);
      if (!h.has("content-type")) h.set("content-type", "application/json");
    }
    const res = await worker.fetch(new Request(url, { method, headers: h, body: body === undefined ? undefined : JSON.stringify(body) }), fullEnv);
    return { status: res.status, body: await res.json(), headers: res.headers };
  }
  return { gh, oauth, worker, call, env: fullEnv, certFetches: () => certFetches };
}

export const save = (call, as, entry, changes, title = "") => call("POST", "save", { as, body: { entry, title, changes } });
