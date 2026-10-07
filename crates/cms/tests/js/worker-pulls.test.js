// The Worker against a fake GitHub: the drafts' pull requests (labelled fugo-cms), a token that
// may not open them, and the drafts Decap CMS left (branches `cms/<collection>/<slug>` with their
// pull requests).

import { test } from "node:test";
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";

import { draftId } from "../../assets/worker.js";
import { ORIGIN, setup, save } from "./worker-setup.js";

const post = "content/blog/post.md";

test("a draft's first save opens its pull request, labelled fugo-cms; publishing closes it", async () => {
  const { call, gh } = setup();
  await save(call, "writer@example.com", "blog/post", [{ path: post, content: "one" }], "Post 2");
  await save(call, "writer2@example.com", "blog/post", [{ path: post, content: "two" }], "Post 2");
  const id = await draftId("blog/post");
  assert.equal(gh.pulls.length, 1, "one pull request per draft");
  const pull = gh.pulls[0];
  assert.equal(pull.head, `cms/${id}`);
  assert.equal(pull.base, "main");
  assert.equal(pull.title, "Edit blog/post: Post 2");
  assert.deepEqual(pull.labels, ["fugo-cms"]);
  assert.ok(pull.body.includes(`${ORIGIN}/admin/#/d/${id}`), pull.body);

  const list = await call("GET", "drafts");
  assert.deepEqual(list.body.drafts[0].pr, { number: 1, url: "https://github.com/owner/site/pull/1", title: "Edit blog/post: Post 2" });
  assert.equal(list.body.drafts[0].title, "Post 2", "the page's title");
  const detail = await call("GET", "draft", { query: { id } });
  assert.equal(detail.body.pr.number, 1);

  const p = await call("POST", "publish", { as: "boss@example.com", body: { id } });
  assert.equal(p.status, 200, JSON.stringify(p.body));
  assert.equal(pull.state, "closed");
  assert.deepEqual(pull.comments, [`Published with the site's editor in ${gh.head("main")}.`]);
});

test("a token that may not open pull requests: drafts work as before, without them", async () => {
  const { call, gh } = setup();
  gh.pullAccess = false;
  const r = await save(call, "writer@example.com", "blog/post", [{ path: post, content: "x" }], "Post");
  assert.equal(r.status, 200, JSON.stringify(r.body));
  assert.equal(gh.pulls.length, 0);
  const list = await call("GET", "drafts");
  assert.equal(list.status, 200, JSON.stringify(list.body));
  assert.equal(list.body.drafts[0].pr, null);
  const detail = await call("GET", "draft", { query: { id: r.body.draft } });
  assert.equal(detail.status, 200, JSON.stringify(detail.body));
  assert.equal(detail.body.pr, null);
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id: r.body.draft } });
  assert.equal(p.status, 200, JSON.stringify(p.body));
  assert.equal(gh.text("main", post), "x");
});

/** A draft of Decap CMS: its branch from main with one commit, and its pull request. */
function decapDraft(gh, branch, files, subject, labels = ["decap-cms/draft"]) {
  gh.refs.set(branch, gh.head("main"));
  gh.push(branch, files, subject, { name: "Ann", email: "writer@example.com" });
  return labels ? gh.openPull(branch, { title: subject, labels }) : null;
}

test("Decap CMS's drafts: listed with their pull requests, shown and published", async () => {
  const { call, gh } = setup();
  const file = "content/cake/tokyo-hiyoko/index.en.md";
  const pull = decapDraft(gh, "cms/cake/tokyo-hiyoko/index", { [file]: "hiyoko" }, "Create cake “tokyo-hiyoko/index”");
  decapDraft(gh, "cms/companies/ja-yubari/_index", { "content/companies/ja-yubari/_index.md": "x" }, "Update companies “ja-yubari/_index”", null);

  const list = await call("GET", "drafts");
  assert.equal(list.status, 200, JSON.stringify(list.body));
  const byId = Object.fromEntries(list.body.drafts.map((d) => [d.id, d]));
  const cake = byId["cake/tokyo-hiyoko/index"];
  assert.equal(cake.entry, "");
  assert.equal(cake.title, "Create cake “tokyo-hiyoko/index”");
  assert.equal(cake.pr.number, pull.number);
  assert.equal(cake.author.email, "writer@example.com");
  const company = byId["companies/ja-yubari/_index"];
  assert.equal(company.title, "Update companies “ja-yubari/_index”", "the commit's subject, without a pull request");
  assert.equal(company.pr, null);

  const detail = await call("GET", "draft", { query: { id: "cake/tokyo-hiyoko/index" } });
  assert.equal(detail.status, 200, JSON.stringify(detail.body));
  assert.deepEqual(detail.body.files.map((f) => [f.path, f.status]), [[file, "added"]]);
  assert.equal(detail.body.pr.number, pull.number);

  const p = await call("POST", "publish", { as: "boss@example.com", body: { id: "cake/tokyo-hiyoko/index" } });
  assert.equal(p.status, 200, JSON.stringify(p.body));
  assert.equal(gh.text("main", file), "hiyoko");
  assert.equal(gh.head("cms/cake/tokyo-hiyoko/index"), undefined);
  assert.equal(pull.state, "closed");
  assert.equal(pull.comments.length, 1);
});

test("Decap CMS's drafts: discarded, and stale ones conflict", async () => {
  const { call, gh } = setup();
  const en = "content/almonds/honey/index.en.md";
  const pull = decapDraft(gh, "cms/almonds/honey/index", { [en]: "decap" }, "Update almonds “honey/index”");
  gh.push("main", { [en]: null, "content/snacks/almonds/honey/index.en.md": "moved" });
  const detail = await call("GET", "draft", { query: { id: "almonds/honey/index" } });
  assert.deepEqual(detail.body.conflicts, [en]);
  const own = await call("POST", "discard", { as: "writer@example.com", body: { id: "almonds/honey/index" } });
  assert.equal(own.status, 200, JSON.stringify(own.body));
  assert.equal(gh.head("cms/almonds/honey/index"), undefined);
  assert.equal(pull.state, "closed");
});

test("a page opened from a draft saves to it, while it exists", async () => {
  const { call, gh } = setup();
  const en = "content/almonds/honey/index.en.md";
  const pull = decapDraft(gh, "cms/almonds/honey/index", { [en]: "decap" }, "Update almonds “honey/index”");
  const id = "almonds/honey/index";
  const f = await call("GET", "file", { query: { path: en, draft: id } });
  const edit = (content, draft, base) => call("POST", "save", { body: { entry: "almonds/honey", title: "Honey", draft, changes: [{ path: en, content, base }] } });
  const r = await edit("edited", id, f.body.sha);
  assert.equal(r.status, 200, JSON.stringify(r.body));
  assert.equal(r.body.draft, id);
  assert.equal(gh.text(`cms/${id}`, en), "edited");
  assert.equal(gh.head(`cms/${await draftId("almonds/honey")}`), undefined, "no second draft");
  assert.deepEqual(gh.pulls, [pull], "no second pull request");

  await call("POST", "discard", { as: "boss@example.com", body: { id } });
  const own = await edit("again", id);
  assert.equal(own.status, 200, JSON.stringify(own.body));
  assert.equal(own.body.draft, await draftId("almonds/honey"), "the page's own draft, once that one is gone");
  assert.equal((await edit("x", "../main")).status, 400);
});

test("draft ids that are not branches under cms/ are refused", async () => {
  const { call } = setup();
  for (const id of ["../main", "a/../../main", "a/./b", "a//b", "a/", ".hidden", "a b", "a?b", "x".repeat(201)]) {
    const r = await call("GET", "draft", { query: { id } });
    assert.equal(r.status, 400, `${id}: ${JSON.stringify(r.body)}`);
  }
  const thai = await call("GET", "draft", { query: { id: "ขนม/น้ำผึ้ง" } });
  assert.equal(thai.status, 404, "a Decap CMS slug in Thai is an id");
});

test("GitHub App: the token may open pull requests when the installation may", async () => {
  const { privateKey, publicKey } = generateKeyPairSync("rsa", {
    modulusLength: 2048,
    privateKeyEncoding: { type: "pkcs1", format: "pem" },
    publicKeyEncoding: { type: "spki", format: "pem" },
  });
  for (const [permissions, granted] of [
    [{ contents: "write" }, { contents: "write" }],
    [{ contents: "write", pull_requests: "write", workflows: "write" }, { contents: "write", pull_requests: "write" }],
  ]) {
    const { call, gh } = setup({ env: { CMS_GITHUB_TOKEN: "", CMS_GITHUB_APP_ID: "123", CMS_GITHUB_APP_KEY: privateKey } });
    gh.allowApp(123, publicKey, 77, permissions);
    const r = await call("GET", "file", { query: { path: post } });
    assert.equal(r.status, 200, JSON.stringify(r.body));
    assert.deepEqual(gh.app.granted, granted);
  }
});
