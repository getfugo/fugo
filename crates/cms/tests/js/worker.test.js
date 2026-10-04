// The Worker against a fake GitHub and a fake Cloudflare Access team: sign-in, roles, the areas,
// saving, drafts, publishing, discarding, the direct workflow and GitHub App tokens.

import { test } from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { pathToFileURL } from "node:url";

import { createWorker, draftId, parseUsers, pkcs1ToPkcs8 } from "../../assets/worker.js";
import { textToBase64, parseTrailers, base64ToText } from "../../assets/worker.js";
import { FakeGitHub } from "./fake-github.js";

const ORIGIN = "https://snack.example";
const TEAM = "https://team.cloudflareaccess.com";
const AUD = "aud-1";
const NOW = Date.parse("2026-06-01T12:00:00Z");

const SETTINGS = {
  version: 1,
  path: "/admin/",
  api: "/admin/api/",
  site: `${ORIGIN}/`,
  workflow: "review",
  git: { host: "github", repo: "owner/site", branch: "main", dir: "" },
  login: { provider: "cloudflare-access", team: TEAM, aud: [AUD] },
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

const FILES = {
  "content/almonds/honey/index.en.md": "---\ntitle: Honey\n---\nEnglish\n",
  "content/almonds/honey/index.th.md": "---\ntitle: น้ำผึ้ง\n---\nไทย\n",
  "content/blog/post.md": "---\ntitle: Post\n---\n",
  "content/books/_content.html": "{# adapter #}",
  "content/big.md": `---\ntitle: Big\n---\n${"x".repeat(3000)}`,
  "config.toml": "baseURL = 'x'\n",
  "layouts/home.html": "<p>x</p>\n",
  "data/snacks.yaml": "a: 1\n",
};

const USERS = {
  "writer@example.com": ["writer"],
  "writer2@example.com": ["writer"],
  "Translator@Example.com": "translator",
  "boss@example.com": ["publisher"],
  "@team.example": ["writer"],
  "ghost@example.com": ["no-such-role"],
};

// ── Access keys and tokens ──

const keys = await crypto.subtle.generateKey(
  { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
  true,
  ["sign", "verify"],
);
const otherKeys = await crypto.subtle.generateKey(
  { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
  true,
  ["sign", "verify"],
);
const publicJwk = { ...(await crypto.subtle.exportKey("jwk", keys.publicKey)), kid: "k1", alg: "RS256", use: "sig" };

const b64url = (bytes) => Buffer.from(bytes).toString("base64url");
async function token(claims = {}, { key = keys.privateKey, kid = "k1", alg = "RS256" } = {}) {
  const header = b64url(JSON.stringify({ alg, kid, typ: "JWT" }));
  const body = b64url(
    JSON.stringify({ iss: TEAM, aud: [AUD], exp: NOW / 1000 + 3600, iat: NOW / 1000, email: "writer@example.com", ...claims }),
  );
  const sig = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, new TextEncoder().encode(`${header}.${body}`));
  return `${header}.${body}.${b64url(new Uint8Array(sig))}`;
}

// ── A Worker over the fakes ──

function setup({ settings = SETTINGS, files = FILES, env = {}, index } = {}) {
  const gh = new FakeGitHub({ files });
  let certFetches = 0;
  const fetch = async (input, init) => {
    const url = typeof input === "string" ? input : input.url;
    if (url === `${TEAM}/cdn-cgi/access/certs`) {
      certFetches++;
      return Response.json({ keys: [publicJwk], public_cert: { kid: "k1" } });
    }
    return gh.fetch(input, init);
  };
  const worker = createWorker(settings, { fetch, now: () => NOW, index });
  const fullEnv = { CMS_USERS: JSON.stringify(USERS), CMS_GITHUB_TOKEN: "test-token", ...env };
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
  return { gh, worker, call, env: fullEnv, certFetches: () => certFetches };
}

const save = (call, as, entry, changes, title = "") => call("POST", "save", { as, body: { entry, title, changes } });

// ── Sign-in ──

test("me: roles, edit globs and publish of the signed-in user", async () => {
  const { call } = setup();
  const r = await call("GET", "me");
  assert.equal(r.status, 200);
  assert.equal(r.headers.get("cache-control"), "no-store");
  assert.deepEqual(r.body.roles, ["writer"]);
  assert.equal(r.body.publish, false);
  assert.equal(r.body.workflow, "review");
  assert.deepEqual(r.body.deny, ["**/_content.*"]);
  const boss = await call("GET", "me", { as: "Boss@Example.com" });
  assert.equal(boss.body.email, "boss@example.com");
  assert.equal(boss.body.publish, true);
  const translator = await call("GET", "me", { as: "translator@example.com" });
  assert.deepEqual(translator.body.roles, ["translator"]);
  const domain = await call("GET", "me", { as: "ann@team.example" });
  assert.deepEqual(domain.body.roles, ["writer"]);
});

test("sign-in failures", async () => {
  const { call } = setup();
  const cases = [
    [{ jwt: null }, 401, /not signed in/],
    [{ jwt: "x.y" }, 401, /not a JWT/],
    [{ jwt: await token({}, { key: otherKeys.privateKey }) }, 401, /bad signature/],
    [{ jwt: await token({ aud: ["other"] }) }, 401, /another application/],
    [{ jwt: await token({ iss: "https://evil.cloudflareaccess.com" }) }, 401, /another team/],
    [{ jwt: await token({ exp: NOW / 1000 - 3600 }) }, 401, /expired/],
    [{ jwt: await token({ nbf: NOW / 1000 + 3600 }) }, 401, /not valid yet/],
    [{ jwt: await token({}, { alg: "HS256" }) }, 401, /algorithm/],
    [{ jwt: await token({ email: undefined, common_name: "svc" }) }, 403, /service tokens/],
    [{ as: "stranger@example.com" }, 403, /no role/],
    [{ as: "ghost@example.com" }, 403, /no role/],
  ];
  for (const [opts, status, message] of cases) {
    const r = await call("GET", "me", opts);
    assert.equal(r.status, status, JSON.stringify(r.body));
    assert.match(r.body.error, message);
  }
});

test("CMS_DEV_USER signs in requests to localhost without a token (wrangler dev), nothing else", async () => {
  const { worker, env } = setup();
  const devEnv = { ...env, CMS_DEV_USER: "Boss@Example.com" };
  const local = await worker.fetch(new Request("http://localhost:8787/admin/api/me"), devEnv);
  assert.equal(local.status, 200);
  assert.equal((await local.json()).email, "boss@example.com");
  const loopback = await worker.fetch(new Request("http://127.0.0.1:8787/admin/api/me"), devEnv);
  assert.equal(loopback.status, 200);
  const deployed = await worker.fetch(new Request(`${ORIGIN}/admin/api/me`), devEnv);
  assert.equal(deployed.status, 401, "only on localhost");
  const unset = await worker.fetch(new Request("http://localhost:8787/admin/api/me"), env);
  assert.equal(unset.status, 401, "only with CMS_DEV_USER");
});

test("Access keys are fetched once, and again only for an unknown key after a minute", async () => {
  const { call, certFetches } = setup();
  await call("GET", "me");
  await call("GET", "me");
  assert.equal(certFetches(), 1);
  const r = await call("GET", "me", { jwt: await token({}, { kid: "rotated" }) });
  assert.equal(r.status, 401);
  assert.equal(certFetches(), 1, "refetched within a minute");
});

test("missing secrets are reported", async () => {
  const { call } = setup({ env: { CMS_USERS: "", CMS_GITHUB_TOKEN: "" } });
  const r = await call("GET", "me");
  assert.equal(r.status, 500);
  assert.match(r.body.error, /CMS_USERS/);
  const { call: call2 } = setup({ env: { CMS_GITHUB_TOKEN: "" } });
  const f = await call2("GET", "file", { query: { path: "content/blog/post.md" } });
  assert.equal(f.status, 500);
  assert.match(f.body.error, /CMS_GITHUB_TOKEN/);
});

test("the content index is for signed-in people only", async () => {
  const { call } = setup({ index: '{"title":"Snacks","entries":[]}' });
  const r = await call("GET", "site");
  assert.equal(r.status, 200);
  assert.deepEqual(r.body, { title: "Snacks", entries: [] });
  assert.equal(r.headers.get("cache-control"), "no-store");
  const anonymous = await call("GET", "site", { jwt: null });
  assert.equal(anonymous.status, 401);
  const stranger = await call("GET", "site", { as: "stranger@example.com" });
  assert.equal(stranger.status, 403);
});

test("bodies are read only after sign-in, and only up to the limit", async () => {
  const { worker, env } = setup();
  let pulled = 0;
  const endless = new ReadableStream({
    pull(c) {
      pulled++;
      c.enqueue(new Uint8Array(64 * 1024));
    },
  });
  const anonymous = await worker.fetch(
    new Request(`${ORIGIN}/admin/api/save`, { method: "POST", body: endless, duplex: "half", headers: { origin: ORIGIN, "content-type": "application/json" } }),
    env,
  );
  assert.equal(anonymous.status, 401);
  assert.ok(pulled <= 1, `read ${pulled} chunks of an anonymous body`);
  const big = new ReadableStream({
    pull(c) {
      c.enqueue(new Uint8Array(64 * 1024));
    },
  });
  const signed = await worker.fetch(
    new Request(`${ORIGIN}/admin/api/save`, {
      method: "POST",
      body: big,
      duplex: "half",
      headers: { origin: ORIGIN, "content-type": "application/json", "cf-access-jwt-assertion": await token() },
    }),
    env,
  );
  assert.equal(signed.status, 413, "a chunked body without a length stops at the limit");
});

test("posts must be same-origin JSON", async () => {
  const { call } = setup();
  const cross = await call("POST", "save", { headers: { origin: "https://evil.example" }, body: {} });
  assert.equal(cross.status, 403);
  assert.match(cross.body.error, /cross-site/);
  const form = await call("POST", "save", { headers: { "content-type": "application/x-www-form-urlencoded" }, body: {} });
  assert.equal(form.status, 415);
  const other = await call("DELETE", "save");
  assert.equal(other.status, 405);
});

test("requests outside the API go to the static files", async () => {
  const { worker } = setup();
  let served = null;
  const res = await worker.fetch(new Request(`${ORIGIN}/almonds/`), { ASSETS: { fetch: async (r) => ((served = r.url), new Response("page")) } });
  assert.equal(await res.text(), "page");
  assert.equal(served, `${ORIGIN}/almonds/`);
  const none = await worker.fetch(new Request(`${ORIGIN}/x`), {});
  assert.equal(none.status, 404);
});

// ── Reading ──

test("file: reads what the areas allow, nothing else", async () => {
  const { call } = setup();
  const r = await call("GET", "file", { query: { path: "content/almonds/honey/index.en.md" } });
  assert.equal(r.status, 200);
  assert.equal(base64ToText(r.body.content), FILES["content/almonds/honey/index.en.md"]);
  const big = await call("GET", "file", { query: { path: "content/big.md" } });
  assert.equal(base64ToText(big.body.content), FILES["content/big.md"], "large files come from the blob API");
  for (const path of ["config.toml", "layouts/home.html", "content/books/_content.html", "content/.env", "../x.md", "/etc/passwd", "content/a/../b.md"]) {
    const denied = await call("GET", "file", { query: { path } });
    assert.equal(denied.status, 403, path);
  }
  const missing = await call("GET", "file", { query: { path: "content/nope.md" } });
  assert.equal(missing.status, 404);
});

// ── Saving ──

test("save: the first save makes the page's draft branch, the next adds to it", async () => {
  const { call, gh } = setup();
  const path = "content/almonds/honey/index.en.md";
  const file = await call("GET", "file", { query: { path } });
  const r = await save(call, "writer@example.com", "almonds/honey", [{ path, content: "---\ntitle: Honey 2\n---\nEnglish\n", base: file.body.sha }], "Honey 2");
  assert.equal(r.status, 200, JSON.stringify(r.body));
  const id = await draftId("almonds/honey");
  assert.equal(r.body.draft, id);
  assert.match(id, /^almonds-honey-[0-9a-f]{8}$/);
  assert.equal(gh.text(`cms/${id}`, path), "---\ntitle: Honey 2\n---\nEnglish\n");
  assert.equal(gh.text("main", path), FILES[path], "main is untouched");
  const commit = gh.commit(`cms/${id}`);
  assert.deepEqual(commit.author, { name: "writer", email: "writer@example.com" });
  assert.deepEqual(parseTrailers(commit.message), { "cms-entry": "almonds/honey", "cms-title": "Honey 2" });

  // Reading through the draft, then saving again on top of it.
  const draftFile = await call("GET", "file", { query: { path, draft: id } });
  assert.equal(base64ToText(draftFile.body.content), "---\ntitle: Honey 2\n---\nEnglish\n");
  const th = "content/almonds/honey/index.th.md";
  const thFile = await call("GET", "file", { query: { path: th, draft: id } });
  const r2 = await save(call, "translator@example.com", "almonds/honey", [{ path: th, content: "---\ntitle: น้ำผึ้ง 2\n---\n", base: thFile.body.sha }]);
  assert.equal(r2.status, 200, JSON.stringify(r2.body));
  assert.equal(gh.text(`cms/${id}`, th), "---\ntitle: น้ำผึ้ง 2\n---\n");
  assert.equal(gh.text(`cms/${id}`, path), "---\ntitle: Honey 2\n---\nEnglish\n");
  assert.equal(gh.commit(`cms/${id}`).author.email, "translator@example.com");
});

test("save: a file changed since it was opened is a conflict", async () => {
  const { call, gh } = setup();
  const path = "content/blog/post.md";
  const file = await call("GET", "file", { query: { path } });
  gh.push("main", { [path]: "---\ntitle: Changed elsewhere\n---\n" });
  const r = await save(call, "writer@example.com", "blog/post", [{ path, content: "mine", base: file.body.sha }]);
  assert.equal(r.status, 409);
  assert.deepEqual(r.body.stale, [path]);
  const created = await save(call, "writer@example.com", "blog/new", [{ path: "content/blog/post.md", content: "new", base: null }]);
  assert.equal(created.status, 409, "base null: the file must not exist yet");
});

test("save: roles and areas decide what may be written", async () => {
  const { call, gh } = setup();
  const one = (path, extra = {}) => [{ path, content: "x", ...extra }];
  const cases = [
    ["translator@example.com", one("content/almonds/honey/index.en.md"), 403],
    ["writer@example.com", one("content/almonds/honey/index.th.md"), 403],
    ["writer@example.com", one("layouts/home.html"), 403],
    ["boss@example.com", one("layouts/home.html"), 403],
    ["boss@example.com", one("config.toml"), 403],
    ["boss@example.com", one("config/production/params.toml"), 403],
    ["boss@example.com", one("content/books/_content.html"), 403],
    ["boss@example.com", one("content/books/_Content.md"), 403],
    ["boss@example.com", one("content/books/_CONTENT.TH.MD"), 403],
    ["boss@example.com", one("content/books/x.html", { content: "<script>x()</script>" }), 403],
    ["boss@example.com", one("content/\ud800.md"), 400],
    ["boss@example.com", one(".github/workflows/x.yml"), 400],
    ["boss@example.com", one("content/x.svg", { encoding: "base64", content: textToBase64("<svg/>") }), 403],
    ["writer@example.com", one("content/blog/../../layouts/x.md"), 400],
    ["boss@example.com", [{ path: "content/a.md", content: "a" }, { path: "content/a.md", content: "b" }], 400],
    ["boss@example.com", one("content/a.png", { encoding: "base64", content: "not base64!" }), 400],
    ["boss@example.com", one("content/a.md", { content: "x".repeat(70 * 1024) }), 413],
  ];
  for (const [as, changes, status] of cases) {
    const r = await save(call, as, "x", changes);
    assert.equal(r.status, status, `${as} ${changes[0].path}: ${JSON.stringify(r.body)}`);
  }
  const ok = await save(call, "boss@example.com", "settings", [{ path: "config/_default/params.toml", content: "x = 1\n" }]);
  assert.equal(ok.status, 200, JSON.stringify(ok.body));
  const upload = await save(call, "writer@example.com", "almonds/honey", [
    { path: "content/almonds/honey/photo.png", content: Buffer.from([137, 80, 78, 71]).toString("base64"), encoding: "base64", base: null },
  ]);
  assert.equal(upload.status, 200, JSON.stringify(upload.body));
  const id = await draftId("almonds/honey");
  assert.deepEqual([...gh.blobs.get(gh.trees.get(gh.commit(`cms/${id}`).tree).get("content/almonds/honey/photo.png"))], [137, 80, 78, 71]);
  const empty = await save(call, "boss@example.com", "x", []);
  assert.equal(empty.status, 400);
  const noEntry = await save(call, "boss@example.com", "", [{ path: "content/a.md", content: "a" }]);
  assert.equal(noEntry.status, 400);
});

test("save: deleting files", async () => {
  const { call, gh } = setup();
  const path = "content/blog/post.md";
  const file = await call("GET", "file", { query: { path } });
  const r = await save(call, "writer@example.com", "blog/post", [{ path, delete: true, base: file.body.sha }]);
  assert.equal(r.status, 200, JSON.stringify(r.body));
  assert.equal(gh.text(`cms/${r.body.draft}`, path), undefined);
  assert.match(gh.commit(`cms/${r.body.draft}`).message, /^Delete blog\/post/);
});

// ── Drafts, publishing, discarding ──

test("drafts and a draft's changes", async () => {
  const { call } = setup();
  const path = "content/blog/post.md";
  await save(call, "writer@example.com", "blog/post", [{ path, content: "---\ntitle: Post 2\n---\n" }], "Post 2");
  const list = await call("GET", "drafts");
  assert.equal(list.status, 200);
  assert.equal(list.body.drafts.length, 1);
  const d = list.body.drafts[0];
  assert.equal(d.entry, "blog/post");
  assert.equal(d.title, "Post 2");
  assert.equal(d.author.email, "writer@example.com");
  const detail = await call("GET", "draft", { query: { id: d.id } });
  assert.equal(detail.status, 200);
  assert.deepEqual(detail.body.files.map((f) => [f.path, f.status]), [[path, "modified"]]);
  assert.match(detail.body.files[0].patch, /\+---/);
  assert.deepEqual(detail.body.conflicts, []);
  const bad = await call("GET", "draft", { query: { id: "../main" } });
  assert.equal(bad.status, 400);
  const gone = await call("GET", "draft", { query: { id: "nope-00000000" } });
  assert.equal(gone.status, 404);
});

test("publish: copies the draft onto the branch in one commit and deletes the draft", async () => {
  const { call, gh } = setup();
  const en = "content/almonds/honey/index.en.md";
  const th = "content/almonds/honey/index.th.md";
  await save(call, "writer@example.com", "almonds/honey", [{ path: en, content: "EN2" }], "Honey");
  await save(call, "translator@example.com", "almonds/honey", [{ path: th, content: "TH2" }], "Honey");
  const id = await draftId("almonds/honey");
  const denied = await call("POST", "publish", { as: "writer@example.com", body: { id } });
  assert.equal(denied.status, 403);
  const mainBefore = gh.head("main");
  const r = await call("POST", "publish", { as: "boss@example.com", body: { id } });
  assert.equal(r.status, 200, JSON.stringify(r.body));
  assert.equal(r.body.published, true);
  assert.equal(gh.text("main", en), "EN2");
  assert.equal(gh.text("main", th), "TH2");
  assert.equal(gh.head(`cms/${id}`), undefined, "the draft is deleted");
  const c = gh.commit("main");
  assert.deepEqual(c.parents, [mainBefore], "one commit on top of the branch");
  assert.equal(c.author.email, "writer@example.com");
  assert.match(c.message, /^Publish almonds\/honey: Honey\n/);
  assert.match(c.message, /Co-authored-by: translator <translator@example.com>/);
  assert.match(c.message, /CMS-Published-By: boss@example.com/);
  const again = await call("POST", "publish", { as: "boss@example.com", body: { id } });
  assert.equal(again.status, 404);
});

test("publish: deletions, and files the branch changed meanwhile", async () => {
  const { call, gh } = setup();
  const post = "content/blog/post.md";
  const del = await save(call, "writer@example.com", "blog/post", [{ path: post, delete: true }]);
  gh.push("main", { "content/blog/other.md": "unrelated" });
  const ok = await call("POST", "publish", { as: "boss@example.com", body: { id: del.body.draft } });
  assert.equal(ok.status, 200, JSON.stringify(ok.body));
  assert.equal(gh.text("main", post), undefined);
  assert.equal(gh.text("main", "content/blog/other.md"), "unrelated", "keeps the branch's other changes");

  const en = "content/almonds/honey/index.en.md";
  const r = await save(call, "writer@example.com", "almonds/honey", [{ path: en, content: "draft" }]);
  gh.push("main", { [en]: "changed on main" });
  const conflict = await call("POST", "publish", { as: "boss@example.com", body: { id: r.body.draft } });
  assert.equal(conflict.status, 409);
  assert.deepEqual(conflict.body.conflicts, [en]);
  const detail = await call("GET", "draft", { query: { id: r.body.draft } });
  assert.deepEqual(detail.body.conflicts, [en]);
  assert.equal(gh.text("main", en), "changed on main");
});

test("publish: conflicts are found however many files the branch changed", async () => {
  const { call, gh } = setup();
  const en = "content/almonds/honey/index.en.md";
  const r = await save(call, "writer@example.com", "almonds/honey", [{ path: en, content: "draft" }]);
  const many = Object.fromEntries(Array.from({ length: 20 }, (_, i) => [`content/aaa/${i}.md`, "x"]));
  gh.push("main", { ...many, [en]: "changed on main" });
  gh.compareLimit = 5;
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id: r.body.draft } });
  assert.equal(p.status, 409, JSON.stringify(p.body));
  assert.deepEqual(p.body.conflicts, [en]);
});

test("publish: a save that lands while publishing stays a draft", async () => {
  const { call, gh } = setup();
  const en = "content/almonds/honey/index.en.md";
  const r = await save(call, "writer@example.com", "almonds/honey", [{ path: en, content: "first" }]);
  const branch = `cms/${r.body.draft}`;
  gh.after = (method, path) => {
    if (method === "PATCH" && path === "/git/refs/heads/main") {
      gh.after = null;
      gh.push(branch, { [en]: "second" }, "late save", { name: "writer", email: "writer@example.com" });
    }
  };
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id: r.body.draft } });
  assert.equal(p.status, 200, JSON.stringify(p.body));
  assert.equal(p.body.kept, true);
  assert.equal(gh.text("main", en), "first");
  assert.equal(gh.text(branch, en), "second", "the late save is not lost");
});

test("publish: a draft that changes files outside what the publisher may write is refused", async () => {
  const { call, gh } = setup();
  const r = await save(call, "writer@example.com", "blog/post", [{ path: "content/blog/post.md", content: "x" }]);
  gh.push(`cms/${r.body.draft}`, { "layouts/home.html": "<script>evil()</script>" });
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id: r.body.draft } });
  assert.equal(p.status, 403);
  assert.match(p.body.error, /layouts\/home.html/);
  assert.equal(gh.text("main", "layouts/home.html"), FILES["layouts/home.html"]);
});

test("discard: your own draft, or any draft when you may publish", async () => {
  const { call, gh } = setup();
  const own = await save(call, "writer@example.com", "blog/post", [{ path: "content/blog/post.md", content: "x" }]);
  const d = await call("POST", "discard", { as: "writer@example.com", body: { id: own.body.draft } });
  assert.equal(d.status, 200, JSON.stringify(d.body));
  assert.equal(gh.head(`cms/${own.body.draft}`), undefined);

  const shared = await save(call, "writer@example.com", "blog/post", [{ path: "content/blog/post.md", content: "x" }]);
  await save(call, "writer2@example.com", "blog/post", [{ path: "content/blog/post.md", content: "y" }]);
  const refused = await call("POST", "discard", { as: "writer@example.com", body: { id: shared.body.draft } });
  assert.equal(refused.status, 403);
  const boss = await call("POST", "discard", { as: "boss@example.com", body: { id: shared.body.draft } });
  assert.equal(boss.status, 200);
});

// ── Other settings ──

test("workflow direct: saves are commits to the branch", async () => {
  const { call, gh } = setup({ settings: { ...SETTINGS, workflow: "direct" } });
  const path = "content/blog/post.md";
  const r = await save(call, "writer@example.com", "blog/post", [{ path, content: "direct" }], "Post");
  assert.equal(r.status, 200, JSON.stringify(r.body));
  assert.equal(r.body.draft, null);
  assert.equal(gh.text("main", path), "direct");
  assert.equal(gh.commit("main").author.email, "writer@example.com");
  const me = await call("GET", "me", { as: "boss@example.com" });
  assert.equal(me.body.publish, false, "nothing to publish");
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id: "x-00000000" } });
  assert.equal(p.status, 400);
});

test("a project in a directory of the repository", async () => {
  const files = { "site/content/blog/post.md": "---\ntitle: P\n---\n", "content/blog/post.md": "outside" };
  const { call, gh } = setup({ settings: { ...SETTINGS, git: { ...SETTINGS.git, dir: "site/" } }, files });
  const f = await call("GET", "file", { query: { path: "content/blog/post.md" } });
  assert.equal(base64ToText(f.body.content), "---\ntitle: P\n---\n");
  const r = await save(call, "writer@example.com", "blog/post", [{ path: "content/blog/post.md", content: "new", base: f.body.sha }]);
  assert.equal(r.status, 200, JSON.stringify(r.body));
  assert.equal(gh.text(`cms/${r.body.draft}`, "site/content/blog/post.md"), "new");
  assert.equal(gh.text(`cms/${r.body.draft}`, "content/blog/post.md"), "outside");
  const detail = await call("GET", "draft", { query: { id: r.body.draft } });
  assert.deepEqual(detail.body.files.map((x) => x.path), ["content/blog/post.md"]);
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id: r.body.draft } });
  assert.equal(p.status, 200, JSON.stringify(p.body));
  assert.equal(gh.text("main", "site/content/blog/post.md"), "new");
});

test("GitHub App: an installation token from the app's PKCS#1 key, cached", async () => {
  const { privateKey, publicKey } = generateKeyPairSync("rsa", {
    modulusLength: 2048,
    privateKeyEncoding: { type: "pkcs1", format: "pem" },
    publicKeyEncoding: { type: "spki", format: "pem" },
  });
  const { call, gh } = setup({ env: { CMS_GITHUB_TOKEN: "", CMS_GITHUB_APP_ID: "123", CMS_GITHUB_APP_KEY: privateKey.replace(/\n/g, "\\n") } });
  gh.allowApp(123, publicKey);
  const r = await call("GET", "file", { query: { path: "content/blog/post.md" } });
  assert.equal(r.status, 200, JSON.stringify(r.body));
  await call("GET", "file", { query: { path: "content/blog/post.md" } });
  assert.equal(gh.app.issued, 1, "the token is reused");
});

test("pkcs1ToPkcs8 wraps a key as node exports it", () => {
  const { privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
  const pkcs1 = privateKey.export({ type: "pkcs1", format: "der" });
  const pkcs8 = privateKey.export({ type: "pkcs8", format: "der" });
  assert.deepEqual(Buffer.from(pkcs1ToPkcs8(new Uint8Array(pkcs1))), pkcs8);
});

test("draft ids and CMS_USERS", async () => {
  assert.match(await draftId("_index"), /^home-[0-9a-f]{8}$/);
  assert.match(await draftId("almonds/_index"), /^almonds-[0-9a-f]{8}$/);
  assert.match(await draftId("ขนม/ไทย"), /^home-[0-9a-f]{8}$/);
  assert.notEqual(await draftId("a/b-c"), await draftId("a-b/c"));
  const users = parseUsers('{"A@X.com": ["Writer", " publisher "], "@x.com": "a, b"}');
  assert.deepEqual(users.get("a@x.com"), ["writer", "publisher"]);
  assert.deepEqual(users.get("@x.com"), ["a", "b"]);
  assert.throws(() => parseUsers("[]"), /JSON object/);
  assert.throws(() => parseUsers("{"), /not JSON/);
});

test("a published _worker.js: its default export is the Worker of the settings before it", async (t) => {
  const file = process.env.CMS_PUBLISHED_WORKER;
  if (!file) return t.skip("run by cargo test -p ssg-cms, which writes the Worker");
  const worker = (await import(pathToFileURL(file).href)).default;
  const env = { CMS_USERS: JSON.stringify({ "boss@example.com": ["owner"] }), CMS_DEV_USER: "boss@example.com" };
  const site = await worker.fetch(new Request("http://localhost:8787/admin/api/site"), env);
  assert.equal(site.status, 200);
  assert.deepEqual(await site.json(), { title: "Snacks", entries: [] });
  const me = await worker.fetch(new Request("http://localhost:8787/admin/api/me"), env);
  const body = await me.json();
  assert.equal(body.email, "boss@example.com");
  assert.equal(body.publish, true);
  const other = await worker.fetch(new Request("http://localhost:8787/about/"), { ASSETS: { fetch: async () => new Response("static") } });
  assert.equal(await other.text(), "static");
});
