// The Worker against a fake GitHub: where a draft's last save is built and served (`GET build`),
// from the deployments that a host or a CI job makes for the commits of the draft's branch.

import { test } from "node:test";
import assert from "node:assert/strict";

import { setup, save } from "./worker-setup.js";

const post = "content/blog/post.md";
const PREVIEW = "https://cms-post.preview.example";
const LOG = "https://ci.example/runs/1";

/** A draft of the post, saved once: its id and its branch. */
async function draft(call, content = "one") {
  const r = await save(call, "writer@example.com", "blog/post", [{ path: post, content }], "Post");
  assert.equal(r.status, 200, JSON.stringify(r.body));
  return { id: r.body.draft, branch: `cms/${r.body.draft}` };
}

const build = (call, id) => call("GET", "build", { query: id === undefined ? {} : { draft: id } });

test("nothing deploys the draft's branch: no build", async () => {
  const { call, gh } = setup();
  const { id, branch } = await draft(call);
  const r = await build(call, id);
  assert.equal(r.status, 200, JSON.stringify(r.body));
  assert.deepEqual(r.body, { sha: gh.head(branch), state: "none", url: null, log: null, built: null });
});

test("the deployment of the draft's last save: building, then built and where", async () => {
  const { call, gh } = setup();
  const { id, branch } = await draft(call);
  const d = gh.deploy(branch);
  let r = await build(call, id);
  assert.equal(r.body.state, "building", "a deployment without a status yet");
  d.statuses.push({ state: "queued" }, { state: "in_progress", log_url: LOG });
  r = await build(call, id);
  assert.deepEqual(r.body, { sha: gh.head(branch), state: "building", url: null, log: LOG, built: gh.head(branch) });
  d.statuses.push({ state: "success", environment_url: PREVIEW, log_url: LOG });
  r = await build(call, id);
  assert.deepEqual(r.body, { sha: gh.head(branch), state: "ready", url: PREVIEW, log: LOG, built: gh.head(branch) });
});

test("a build that failed: its log", async () => {
  const { call, gh } = setup();
  const { id, branch } = await draft(call);
  gh.deploy(branch, [{ state: "failure", target_url: LOG }]);
  const r = await build(call, id);
  assert.equal(r.body.state, "failed");
  assert.equal(r.body.log, LOG, "target_url when there is no log_url");
});

test("a save not built yet: the build of an earlier save, waiting", async () => {
  const { call, gh } = setup();
  const { id, branch } = await draft(call);
  const first = gh.head(branch);
  gh.deploy(branch, [{ state: "success", environment_url: PREVIEW }]);
  await draft(call, "two");
  assert.notEqual(gh.head(branch), first);
  const r = await build(call, id);
  assert.deepEqual(r.body, { sha: gh.head(branch), state: "waiting", url: PREVIEW, log: null, built: first });
});

test("a deployment that serves nothing gives way to one that does", async () => {
  // A CI job that takes an environment for its secrets makes a deployment without an address.
  const { call, gh } = setup();
  const { id, branch } = await draft(call);
  gh.deploy(branch, [{ state: "success", environment_url: PREVIEW }]);
  gh.deploy(branch, [{ state: "success" }], { environment: "secrets" });
  const r = await build(call, id);
  assert.equal(r.body.state, "ready");
  assert.equal(r.body.url, PREVIEW);
});

test("only http and https addresses are given", async () => {
  const { call, gh } = setup();
  const { id, branch } = await draft(call);
  gh.deploy(branch, [{ state: "success", environment_url: "javascript:alert(1)", log_url: "data:text/html,x" }]);
  const r = await build(call, id);
  assert.equal(r.body.state, "ready");
  assert.equal(r.body.url, null);
  assert.equal(r.body.log, null);
});

test("a token that may not read deployments: no build, and the drafts work as before", async () => {
  const { call, gh } = setup();
  const { id, branch } = await draft(call);
  gh.deploy(branch, [{ state: "success", environment_url: PREVIEW }]);
  gh.deployAccess = false;
  const r = await build(call, id);
  assert.equal(r.status, 200, JSON.stringify(r.body));
  assert.equal(r.body.state, "none");
  assert.equal((await call("GET", "draft", { query: { id } })).status, 200);
});

test("without a draft: the build of the branch's last commit", async () => {
  const { call, gh } = setup();
  gh.deploy("main", [{ state: "success", environment_url: "https://snack.example/" }], { environment: "production" });
  const r = await build(call);
  assert.deepEqual(r.body, { sha: gh.head("main"), state: "ready", url: "https://snack.example/", log: null, built: gh.head("main") });
});

test("a draft that is gone", async () => {
  const { call } = setup();
  const r = await build(call, "blog-post-00000000");
  assert.equal(r.status, 404, JSON.stringify(r.body));
});
