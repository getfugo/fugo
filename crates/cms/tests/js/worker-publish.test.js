// The Worker against a fake GitHub and a fake Cloudflare Access team: drafts, publishing,
// discarding, the direct workflow, projects in a directory and GitHub App tokens.

import { test } from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { pathToFileURL } from "node:url";

import { draftId, parseUsers, pkcs1ToPkcs8, base64ToText } from "../../assets/worker.js";
import { SETTINGS, FILES, token, setup, save } from "./worker-setup.js";

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
