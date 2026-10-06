// The Worker against a fake GitHub and a fake Cloudflare Access team: sign-in, roles, the areas,
// reading and saving (worker-publish.test.js: drafts, publishing and the other settings).

import { test } from "node:test";
import assert from "node:assert/strict";

import { draftId, textToBase64, parseTrailers, base64ToText } from "../../assets/worker.js";
import { ORIGIN, NOW, FILES, keys, otherKeys, token, setup, save } from "./worker-setup.js";

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


test("save: moving a page moves its files, without copying them", async () => {
  const { call, gh } = setup();
  const [from, to] = ["content/almonds/honey", "content/nuts/honey"];
  const en = await call("GET", "file", { query: { path: `${from}/index.en.md` } });
  const r = await save(call, "boss@example.com", "almonds/honey", [
    { path: `${from}/index.en.md`, delete: true, base: en.body.sha },
    { path: `${to}/index.en.md`, content: "---\ntitle: Honey\naliases: [/almonds/honey/]\n---\nEnglish\n", base: null },
    { path: `${from}/index.th.md`, delete: true },
    { path: `${to}/index.th.md`, from: `${from}/index.th.md`, base: null },
  ]);
  assert.equal(r.status, 200, JSON.stringify(r.body));
  const draft = `cms/${r.body.draft}`;
  const tree = gh.trees.get(gh.commit(draft).tree);
  const main = gh.trees.get(gh.commit("main").tree);
  assert.equal(tree.get(`${to}/index.th.md`), main.get(`${from}/index.th.md`), "the moved file keeps its blob");
  assert.equal(tree.get(`${from}/index.th.md`), undefined);
  assert.equal(gh.text(draft, `${to}/index.en.md`), "---\ntitle: Honey\naliases: [/almonds/honey/]\n---\nEnglish\n");
  assert.match(gh.commit(draft).message, /^Move almonds\/honey/);

  // A move deletes its source (no copies), needs the right to change it, and fails when it is gone.
  const copy = await save(call, "boss@example.com", "x", [{ path: "content/blog/copy.md", from: "content/blog/post.md" }]);
  assert.equal(copy.status, 400, JSON.stringify(copy.body));
  const writer = await save(call, "writer@example.com", "x", [
    { path: "content/blog/moved.md", from: "content/almonds/honey/index.th.md" },
    { path: "content/almonds/honey/index.th.md", delete: true },
  ]);
  assert.equal(writer.status, 403, JSON.stringify(writer.body));
  const gone = await save(call, "boss@example.com", "x", [
    { path: "content/blog/gone.md", from: "content/blog/nope.md" },
    { path: "content/blog/nope.md", delete: true },
  ]);
  assert.equal(gone.status, 409, JSON.stringify(gone.body));
  assert.deepEqual(gone.body.stale, ["content/blog/nope.md"]);
});
