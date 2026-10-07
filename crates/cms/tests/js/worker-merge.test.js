// The Worker against a fake GitHub: publishing a draft whose files the branch changed since it was
// made merges the branch into the draft first (the fake merges files, and lines of a file, as git
// does), stops when both changed the same lines, and the merge commits are nobody's saves.

import { test } from "node:test";
import assert from "node:assert/strict";

import { setup, save } from "./worker-setup.js";

const en = "content/almonds/honey/index.en.md";
const page = (title, text) => `---\ntitle: ${title}\n---\n${text}\n`;

/** A draft of almonds/honey that changes the text of its English page, and a push to main since
 * that changes the page to `onMain`. */
async function behind(call, gh, onMain) {
  const r = await save(call, "writer@example.com", "almonds/honey", [{ path: en, content: page("Honey", "English, edited") }], "Honey");
  assert.equal(r.status, 200, JSON.stringify(r.body));
  gh.push("main", { [en]: onMain });
  return { id: r.body.draft, branch: `cms/${r.body.draft}` };
}

/** Changes the page on main once more right after the next merge: publishing stops, and the
 * merge stays on the draft. */
function pushAfterMerge(gh, onMain) {
  gh.after = (method, path) => {
    if (method === "POST" && path === "/merges") {
      gh.after = null;
      gh.push("main", { [en]: onMain });
    }
  };
}

const merges = (gh) => [...gh.commits.values()].filter((c) => c.parents.length > 1);

test("publish: merges the branch into a draft whose files it changed, on other lines", async () => {
  const { call, gh } = setup();
  const { id, branch } = await behind(call, gh, page("Honey 2", "English"));
  const detail = await call("GET", "draft", { query: { id } });
  assert.deepEqual(detail.body.conflicts, [en]);
  const mainBefore = gh.head("main");
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id } });
  assert.equal(p.status, 200, JSON.stringify(p.body));
  assert.equal(p.body.published, true);
  assert.equal(gh.text("main", en), page("Honey 2", "English, edited"), "both changes");
  const c = gh.commit("main");
  assert.deepEqual(c.parents, [mainBefore], "one commit on top of the branch");
  assert.equal(c.author.email, "writer@example.com");
  assert.match(c.message, /^Publish almonds\/honey: Honey\n/);
  assert.doesNotMatch(c.message, /Co-authored-by/, "the bot's merge is nobody's save");
  assert.equal(gh.head(branch), undefined, "the draft is deleted");
  const [merge, ...more] = merges(gh);
  assert.equal(more.length, 0);
  assert.equal(merge.author.email, "bot@example.com");
  assert.match(merge.message, /^Bring in main\n\n/);
  assert.match(merge.message, /\nCMS-Entry: almonds\/honey\n/);
  assert.match(merge.message, /\nCMS-Title: Honey\n/);
});

test("publish: stops when the branch and the draft changed the same lines", async () => {
  const { call, gh } = setup();
  const { id, branch } = await behind(call, gh, page("Honey", "English, on main"));
  const [head, main] = [gh.head(branch), gh.head("main")];
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id } });
  assert.equal(p.status, 409, JSON.stringify(p.body));
  assert.match(p.body.error, /same lines/);
  assert.deepEqual(p.body.conflicts, [en]);
  assert.equal(gh.head("main"), main, "the branch is unchanged");
  assert.equal(gh.head(branch), head, "and the draft: no merge");
  assert.equal(merges(gh).length, 0);
});

test("a draft a merge brought up to date: listed with its page, its saves only, discarded by its writer", async () => {
  const { call, gh } = setup();
  const { id, branch } = await behind(call, gh, page("Honey 2", "English"));
  pushAfterMerge(gh, page("Honey 3", "English"));
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id } });
  assert.equal(p.status, 409, JSON.stringify(p.body));
  assert.deepEqual(p.body.conflicts, [en]);
  assert.equal(gh.commit(branch).parents.length, 2, "the draft's last commit is the merge");

  const list = await call("GET", "drafts");
  assert.deepEqual(
    list.body.drafts.map((d) => [d.id, d.entry, d.title]),
    [[id, "almonds/honey", "Honey"]],
  );
  const detail = await call("GET", "draft", { query: { id } });
  assert.equal(detail.body.title, "Honey");
  assert.deepEqual(
    detail.body.commits.map((c) => [c.author.email, c.subject]),
    [["writer@example.com", "Edit almonds/honey: Honey"]],
  );
  const own = await call("POST", "discard", { as: "writer@example.com", body: { id } });
  assert.equal(own.status, 200, JSON.stringify(own.body));
  assert.equal(gh.head(branch), undefined);
});

test("publish: a draft merged before is merged again", async () => {
  const { call, gh } = setup();
  const { id } = await behind(call, gh, page("Honey 2", "English"));
  pushAfterMerge(gh, page("Honey 3", "English"));
  await call("POST", "publish", { as: "boss@example.com", body: { id } });
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id } });
  assert.equal(p.status, 200, JSON.stringify(p.body));
  assert.equal(gh.text("main", en), page("Honey 3", "English, edited"));
  assert.equal(merges(gh).length, 2);
  const c = gh.commit("main");
  assert.equal(c.author.email, "writer@example.com");
  assert.doesNotMatch(c.message, /Co-authored-by/);
});

test("a draft of Decap CMS a merge brought up to date keeps its title", async () => {
  const { call, gh } = setup();
  const branch = "cms/almonds/honey/index";
  gh.refs.set(branch, gh.head("main"));
  gh.push(branch, { [en]: page("Honey", "English, Decap") }, "Update almonds “honey/index”", { name: "Ann", email: "writer@example.com" });
  gh.push("main", { [en]: page("Honey 2", "English") });
  pushAfterMerge(gh, page("Honey 3", "English"));
  const p = await call("POST", "publish", { as: "boss@example.com", body: { id: "almonds/honey/index" } });
  assert.equal(p.status, 409, JSON.stringify(p.body));
  assert.equal(gh.commit(branch).parents.length, 2);
  const list = await call("GET", "drafts");
  assert.deepEqual(
    list.body.drafts.map((d) => [d.id, d.entry, d.title]),
    [["almonds/honey/index", "", "Update almonds “honey/index”"]],
  );
});
